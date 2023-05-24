mod loc;

use std::error::Error;
use std::io;
use actix_web::{App, HttpServer};
use actix_web_prom::PrometheusMetricsBuilder;
use log::LevelFilter;
use once_cell::sync::Lazy;
use prometheus::{Counter, Opts};
use regex::Regex;
use teloxide::prelude::*;
use teloxide::types::{InlineQueryResult, InlineQueryResultLocation, Me};
use teloxide::types::ParseMode::MarkdownV2;
use crate::loc::{Location, LocFinder};

static CACHE_TIME: Lazy<Option<u32>> = Lazy::new(|| std::env::var("CACHE_TIME")
    .ok()
    .map(|v| { v.parse().ok() })
    .flatten()
);

static COORDS_REGEXP: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?P<latitude>\d{2}\.\d{4,6}),? (?P<longitude>\d{2}\.\d{4,6})")
    .expect("Invalid regex!"));
static FINDER: Lazy<LocFinder> = Lazy::new(|| LocFinder::from_env());

static INLINE_COUNTER: Lazy<Counter> = Lazy::new(|| {
    let counter_opts = Opts::new("inline_usage_total", "count of inline queries processed by the bot");
    Counter::with_opts(counter_opts).expect("unable to create the inline counter")
});
static INLINE_CHOSEN_COUNTER: Lazy<Counter> = Lazy::new(|| {
    let counter_opts = Opts::new("inline_chosen_total", "count of inline results chosen by the users");
    Counter::with_opts(counter_opts).expect("unable to create the inline chosen counter")
});
static MESSAGE_COUNTER: Lazy<Counter> = Lazy::new(|| {
    let counter_opts = Opts::new("message_usage_total", "count of messages processed by the bot");
    Counter::with_opts(counter_opts).expect("unable to create the message counter")
});
static GOOGLE_API_REQUESTS_COUNTER: Lazy<Counter> = Lazy::new(|| {
    let counter_opts = Opts::new("google_maps_api_requests_total", "count of requests to the Google Maps API");
    Counter::with_opts(counter_opts).expect("unable to create the Google Maps API requests counter")
});


#[tokio::main]
async fn main() -> io::Result<()> {
    pretty_env_logger::init();
    log::set_max_level(LevelFilter::Info);

    let prometheus = PrometheusMetricsBuilder::new("bot")
        .endpoint("/metrics")
        .build()
        .expect("unable to build Prometheus metrics");
    prometheus.registry.register(Box::new(INLINE_COUNTER.clone()))
        .expect("unable to register the inline counter");
    prometheus.registry.register(Box::new(INLINE_CHOSEN_COUNTER.clone()))
        .expect("unable to register the inline chosen counter");
    prometheus.registry.register(Box::new(MESSAGE_COUNTER.clone()))
        .expect("unable to register the message counter");
    prometheus.registry.register(Box::new(GOOGLE_API_REQUESTS_COUNTER.clone()))
        .expect("unable to register the Google Maps API requests counter");
    let srv_fut = HttpServer::new(move || App::new().wrap(prometheus.clone()))
        .bind(("0.0.0.0", 8080))?
        .run();

    let bot = Bot::from_env();
    let handler = dptree::entry()
        .branch(Update::filter_inline_query().endpoint(inline_handler))
        .branch(Update::filter_message().endpoint(message_handler))
        .branch(Update::filter_chosen_inline_result().endpoint(inline_chosen_handler));
    let mut bot_fut = Dispatcher::builder(bot, handler)
        .enable_ctrlc_handler()
        .build();
    let bot_fut = bot_fut.dispatch();

    let (res, _) = futures::future::join(srv_fut, bot_fut).await;
    res
}

async fn inline_handler(bot: Bot, q: InlineQuery) -> Result<(), Box<dyn Error + Send + Sync>> {
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

async fn send_locations(bot: Bot, query_id: &str, locations: Vec<Location>) -> Result<(), Box<dyn Error + Send + Sync>> {
    let results: Vec<InlineQueryResult> = locations.iter()
        .map(|l| InlineQueryResult::Location(
            InlineQueryResultLocation::new(l.address(), l.address(), l.latitude(), l.longitude())
        ))
        .collect();

    let mut answer = bot.answer_inline_query(query_id, results);
    answer.cache_time = *CACHE_TIME;
    match answer.await {
        Ok(_) => Ok(()),
        Err(err) => Err(Box::new(err))
    }
}

async fn inline_chosen_handler(_: Bot, _: ChosenInlineResult) -> Result<(), Box<dyn Error + Send + Sync>> {
    INLINE_CHOSEN_COUNTER.inc();
    Ok(())
}

async fn message_handler(bot: Bot, msg: Message, me: Me) -> Result<(), Box<dyn Error + Send + Sync>> {
    MESSAGE_COUNTER.inc();
    let help = format!("Use me via inline queries:\n`@{} Hermitage Russia`", me.username());
    let mut answer = bot.send_message(msg.chat.id, help);
    answer.parse_mode = Some(MarkdownV2);
    answer.await?;
    Ok(())
}
