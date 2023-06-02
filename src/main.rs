mod loc;
mod metrics;

use std::env::VarError;
use std::net::SocketAddr;
use axum::Router;
use once_cell::sync::Lazy;
use regex::Regex;
use reqwest::Url;
use teloxide::prelude::*;
use teloxide::types::{InlineQueryResult, InlineQueryResultLocation, Me};
use teloxide::types::ParseMode::MarkdownV2;
use teloxide::update_listeners::webhooks::{axum_to_router, Options};
use crate::loc::{Location, LocFinder};
use crate::metrics::{
    MESSAGE_COUNTER,
    INLINE_COUNTER,
    INLINE_CHOSEN_COUNTER,
    GOOGLE_API_REQUESTS_COUNTER
};

const ENV_WEBHOOK_URL: &str = "WEBHOOK_URL";

static CACHE_TIME: Lazy<Option<u32>> = Lazy::new(|| std::env::var("CACHE_TIME")
    .ok()
    .map(|v| { v.parse().ok() })
    .flatten()
);

static COORDS_REGEXP: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?P<latitude>\d{2}\.\d{4,6}),? (?P<longitude>\d{2}\.\d{4,6})")
    .expect("Invalid regex!"));
static FINDER: Lazy<LocFinder> = Lazy::new(|| LocFinder::from_env());


#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    pretty_env_logger::init();

    let bot = Bot::from_env();
    let handler = dptree::entry()
        .branch(Update::filter_inline_query().endpoint(inline_handler))
        .branch(Update::filter_message().endpoint(message_handler))
        .branch(Update::filter_chosen_inline_result().endpoint(inline_chosen_handler));

    let webhook_url: Option<Url> = match std::env::var(ENV_WEBHOOK_URL) {
        Ok(env_url) if env_url.len() > 0 => Some(env_url.parse()?),
        Ok(env_url) if env_url.len() == 0 => None,
        Err(VarError::NotPresent) => None,
        _ => Err("invalid webhook URL!")?
    };
    let addr = SocketAddr::from(([0, 0, 0, 0], 8080));
    let metrics_router = metrics::init();
    match webhook_url {
        Some(url) => {
            log::info!("Setting the webhook: {url}");

            let (listener, stop_flag, bot_router) = axum_to_router(bot.clone(), Options::new(addr, url)).await?;

            let error_handler = LoggingErrorHandler::with_custom_text("An error from the update listener");
            let mut dispatcher = Dispatcher::builder(bot, handler).build();
            let bot_fut = dispatcher.dispatch_with_listener(listener, error_handler);

            let srv = tokio::spawn(async move {
                axum::Server::bind(&addr)
                    .serve(Router::new()
                        .merge(metrics_router)
                        .merge(bot_router)
                        .into_make_service())
                    .with_graceful_shutdown(stop_flag)
                    .await
                }
            );

            let (res, _) = futures::join!(srv, bot_fut);
            res?.map_err(|e| e.into()).into()
        }
        None => {
            log::info!("Polling dispatcher is activating...");

            let mut dispatcher = Dispatcher::builder(bot, handler)
                .enable_ctrlc_handler()
                .build();
            let bot_fut = dispatcher.dispatch();

            let srv = axum::Server::bind(&addr)
                .serve(metrics_router.into_make_service());

            let (res, _) = futures::join!(srv, bot_fut);
            res.map_err(|e| e.into())
        }
    }
}

async fn inline_handler(bot: Bot, q: InlineQuery) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    log::info!("Got query: {}", q.query);
    INLINE_COUNTER.inc();

    let locations = if let Some(coords) = COORDS_REGEXP.captures(q.query.as_str()) {
        let lat: f64 = coords["latitude"].parse()?;
        let long: f64 = coords["longitude"].parse()?;
        vec![Location::new(lat, long)]
    } else {
        let lang_code = q.from.language_code
            .unwrap_or_else(|| {
                log::warn!("no language_code for {}, using the default", q.from.id);
                String::from("en")
            });
        GOOGLE_API_REQUESTS_COUNTER.inc();
        FINDER.find(q.query.as_str(), lang_code.as_str()).await?
    };

    send_locations(bot, q.id.as_str(), locations).await
}

async fn send_locations(bot: Bot, query_id: &str, locations: Vec<Location>) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let results: Vec<InlineQueryResult> = locations.iter()
        .map(|l| {
            let uuid = uuid::Uuid::new_v4().to_string();
            let address = l.address().unwrap_or(String::from("Point on the map"));
            InlineQueryResult::Location(
                InlineQueryResultLocation::new(uuid, address, l.latitude(), l.longitude())
        )})
        .collect();

    let mut answer = bot.answer_inline_query(query_id, results);
    answer.cache_time = *CACHE_TIME;
    match answer.await {
        Ok(_) => Ok(()),
        Err(err) => Err(Box::new(err))
    }
}

async fn inline_chosen_handler(_: Bot, _: ChosenInlineResult) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    INLINE_CHOSEN_COUNTER.inc();
    Ok(())
}

async fn message_handler(bot: Bot, msg: Message, me: Me) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    MESSAGE_COUNTER.inc();

    let help = msg.from()
        .and_then(|u| u.language_code.clone())
        .filter(|lang_code| lang_code == "ru")
        .map(|_| format!("Используй меня через режим встроенных запросов:\n`@{} Эрмитаж`", me.username()))
        .unwrap_or(format!("Use me via inline queries:\n`@{} Statue of Liberty`", me.username()));

    let mut answer = bot.send_message(msg.chat.id, help);
    answer.parse_mode = Some(MarkdownV2);
    answer.await?;
    Ok(())
}
