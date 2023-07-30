use once_cell::sync::Lazy;
use regex::Regex;
use teloxide::prelude::*;
use teloxide::types::{InlineQueryResult, InlineQueryResultLocation, Me};
use teloxide::types::ParseMode::MarkdownV2;
use crate::config;
use crate::loc::{Location, LocFinder};
use crate::metrics::{
    MESSAGE_COUNTER,
    INLINE_COUNTER,
    INLINE_CHOSEN_COUNTER,
};

type HandlerResult = Result<(), Box<dyn std::error::Error + Send + Sync>>;

const POINT_ON_MAP_EN: &str = "Point on the map";
const POINT_ON_MAP_RU: &str = "Точка на карте";

static CACHE_TIME: Lazy<Option<u32>> = Lazy::new(|| std::env::var("CACHE_TIME")
    .ok()
    .map(|v| { v.parse().ok() })
    .flatten()
);
static COORDS_REGEXP: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?P<latitude>\d{1,2}(\.\d+)?),? (?P<longitude>\d{1,3}(\.\d+)?)")
    .expect("Invalid regex!"));
static FINDER: Lazy<LocFinder> = Lazy::new(|| LocFinder::from_env());

pub async fn inline_handler(bot: Bot, q: InlineQuery) -> HandlerResult {
    if q.query.is_empty() {
        bot.answer_inline_query(q.id, vec![]).await?;
        return Ok(());
    }

    log::info!("Got query: {}", q.query);
    INLINE_COUNTER.inc();

    let locations = if let Some(coords) = COORDS_REGEXP.captures(q.query.as_str()) {
        let lat: f64 = coords["latitude"].parse()?;
        let long: f64 = coords["longitude"].parse()?;
        vec![Location::new(lat, long)]
    } else {
        let lang_code = q.from.language_code.clone()
            .unwrap_or_else(|| {
                log::warn!("no language_code for {}, using the default", q.from.id);
                String::from("en")
            });
        let addr = q.query.as_str();
        match *config::GAPI_MODE {
            config::GoogleAPIMode::Place => FINDER.find_place(addr, lang_code.as_str()).await?,
            config::GoogleAPIMode::Text => FINDER.find_text(addr, lang_code.as_str()).await?,
            config::GoogleAPIMode::GeoPlace => FINDER.find(addr, lang_code.as_str()).await?,
            config::GoogleAPIMode::GeoText => FINDER.find_more(addr, lang_code.as_str()).await?,
        }
    };

    send_locations(bot, q.id, q.from.language_code, locations).await
}

async fn send_locations(bot: Bot, query_id: String, lang_code: Option<String>, locations: Vec<Location>) -> HandlerResult {
    let results: Vec<InlineQueryResult> = locations.iter()
        .map(|l| {
            let uuid = uuid::Uuid::new_v4().to_string();
            let address = l.address().unwrap_or_else(|| lang_code.clone()
                .filter(|lang_code| lang_code == "ru")
                .map(|_| POINT_ON_MAP_RU)
                .unwrap_or(POINT_ON_MAP_EN)
                .to_string()
            );
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

pub async fn inline_chosen_handler(_: Bot, _: ChosenInlineResult) -> HandlerResult {
    INLINE_CHOSEN_COUNTER.inc();
    Ok(())
}

pub async fn message_handler(bot: Bot, msg: Message, me: Me) -> HandlerResult {
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
