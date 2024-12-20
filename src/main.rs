use teloxide::update_listeners::UpdateListener;
extern crate core;

mod loc;
mod metrics;
mod handlers;
mod help;
mod utils;
mod users;
mod eula;
mod commands;
mod redis;

#[cfg(test)]
mod testutils;

use std::env::VarError;
use std::net::SocketAddr;
use std::time::Duration;
use futures::future::join_all;
use reqwest::Url;
use rust_i18n::i18n;
use teloxide::dispatching::dialogue::RedisStorage;
use teloxide::dispatching::dialogue::serializer::Json;
use teloxide::dptree::deps;
use teloxide::prelude::*;
use teloxide::update_listeners::webhooks::{axum_to_router, Options};
use crate::handlers::options::CancellationCallbackData;
use crate::handlers::options::location::LocationState;
use crate::redis::REDIS;
use crate::users::{Hello, UserService, UserServiceClientGrpc};

const ENV_WEBHOOK_URL: &str = "WEBHOOK_URL";
const ENV_CACHE_CLEAN_UP_INTERVAL_SECS: &str = "CACHE_CLEAN_UP_INTERVAL_SECS";

pub type CommandCacheStorage = RedisStorage<Json>;

i18n!(fallback = "en");    // load localizations with default parameters

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    #[cfg(debug_assertions)]
    dotenvy::dotenv()?;
    
    pretty_env_logger::init();
    handlers::preload_env_vars();

    let handler = dptree::entry()
        .branch(Update::filter_inline_query().endpoint(handlers::inline_handler))
        .branch(Update::filter_chosen_inline_result().endpoint(handlers::inline_chosen_handler))
        .branch(Update::filter_message().filter_command::<handlers::options::location::Commands>().enter_dialogue::<Message, CommandCacheStorage, LocationState>()
            .branch(dptree::case![LocationState::Start].endpoint(handlers::options::location::start)))
        .branch(Update::filter_message().enter_dialogue::<Message, CommandCacheStorage, LocationState>()
            .branch(dptree::case![LocationState::Requested].endpoint(handlers::options::location::requested)))
        .branch(Update::filter_message().filter_command::<handlers::Command>().endpoint(handlers::command_handler))
        .branch(Update::filter_message().endpoint(handlers::message_handler))
        .branch(Update::filter_callback_query().filter(handlers::options::consent::callback_filter).endpoint(handlers::options::consent::callback_handler))
        .branch(Update::filter_callback_query().filter(handlers::options::cancellation_filter::<CancellationCallbackData>).endpoint(handlers::options::cancellation_handler::<LocationState, CancellationCallbackData>))
        .branch(Update::filter_callback_query().endpoint(handlers::callback_handler));

    let bot = Bot::from_env();
    bot.delete_webhook().await?;

    let set_my_commands_requests = _rust_i18n_available_locales()
        .into_iter()
        .map(|locale| commands::set_my_commands(&bot, locale));
    let set_my_commands_failed = join_all(set_my_commands_requests)
        .await
        .into_iter()
        .any(|res| res.is_err());
    if set_my_commands_failed {
        Err("couldn't set the bot's commands")?
    } else {
        log::info!("The commands has been updated successfully!")
    }

    let webhook_url: Option<Url> = match std::env::var(ENV_WEBHOOK_URL) {
        Ok(env_url) if !env_url.is_empty() => Some(env_url.parse()?),
        Ok(env_url) if env_url.is_empty() => None,
        Err(VarError::NotPresent) => None,
        _ => Err("invalid webhook URL!")?
    };
    let addr = SocketAddr::from(([0, 0, 0, 0], 8080));
    let metrics_router = metrics::init();

    let user_service_grpc = UserServiceClientGrpc::with_addr_from_env(Hello::from("LocPlaceBot")).await;
    let user_service = match user_service_grpc {
        Ok(grpc) => {
            let grpc_client = grpc.clone();
            let interval = std::env::var(ENV_CACHE_CLEAN_UP_INTERVAL_SECS)
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(3600);
            let interval = Duration::from_secs(interval);
            tokio::spawn(async move {
                loop {
                    grpc_client.clean_up_cache();
                    tokio::time::sleep(interval).await;
                }
            });
            UserService::Connected(grpc)
        },
        Err(e) => {
            log::error!("couldn't connect to user-service: {e}");
            UserService::Disabled
        }
    };
    let deps = deps![
        user_service,
        RedisStorage::open(&REDIS.connection_url, Json).await?
    ];

    match webhook_url {
        Some(url) => {
            log::info!("Setting a webhook: {url}");

            let (mut listener, stop_flag, bot_router) = axum_to_router(bot.clone(), Options::new(addr, url)).await?;
            let stop_token = listener.stop_token();

            let error_handler = LoggingErrorHandler::with_custom_text("An error from the update listener");
            let mut dispatcher = Dispatcher::builder(bot, handler)
                .dependencies(deps)
                .build();
            let bot_fut = dispatcher.dispatch_with_listener(listener, error_handler);

            let srv = tokio::spawn(async move {
                let tcp_listener = tokio::net::TcpListener::bind(addr)
                    .await
                    .map_err(|err| {
                        stop_token.stop();
                        err
                    })?;
                let app = axum::Router::new()
                    .merge(metrics_router)
                    .merge(bot_router);
                axum::serve(tcp_listener, app)
                    .with_graceful_shutdown(stop_flag)
                    .await
                }
            );

            let (res, _) = futures::join!(srv, bot_fut);
            res?.map_err(Into::into)
        }
        None => {
            log::info!("The polling dispatcher is activating...");

            let bot_fut = tokio::spawn(async move {
                Dispatcher::builder(bot, handler)
                    .dependencies(deps)
                    .enable_ctrlc_handler()
                    .build()
                    .dispatch()
                    .await
            });

            let srv = tokio::spawn(async move {
                let tcp_listener = tokio::net::TcpListener::bind(addr).await?;
                axum::serve(tcp_listener, metrics_router)
                    .with_graceful_shutdown(async {
                        tokio::signal::ctrl_c()
                            .await
                            .expect("failed to install CTRL+C signal handler");
                        log::info!("Shutdown of the metrics server")
                    })
                    .await
            });

            let (res, _) = futures::join!(srv, bot_fut);
            res?.map_err(Into::into)
        }
    }
}
