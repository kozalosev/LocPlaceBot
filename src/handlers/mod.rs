pub mod options;

mod senders;
mod limiter;
mod query;

#[cfg(test)]
mod test;
#[cfg(test)]
mod limiter_test;

use std::clone::Clone;
use std::ops::Not;
use anyhow::anyhow;
use derive_more::From;
use regex::Regex;
use once_cell::sync::Lazy;
use rust_i18n::t;
use crate::{help, metrics};
use crate::loc::{finder, google, osm, yandex, Location, SearchChain};
use crate::utils::{ensure_lang_code, try_determine_location};
use teloxide::prelude::*;
use teloxide::dispatching::dialogue::GetChatId;
use teloxide::types::{Me, ReplyMarkup};
use teloxide::types::ParseMode::{Html, MarkdownV2};
use teloxide::utils::command::BotCommands;
use crate::handlers::limiter::RequestsLimiter;
use crate::handlers::options::LanguageCode;
use crate::handlers::query::{QueryCheckMode, QUERY_CHECK_MODE};
use crate::redis::REDIS;
use crate::users::{UserService, UserServiceClient, UserServiceClientGrpc};

#[derive(BotCommands, Clone)]
#[command(rename_rule = "lowercase")]
pub enum Command {
    #[command(description = "help")]
    Help,
    Start,
    #[command(description = "loc")]
    Loc,
    SetLanguage(LanguageCode),
    #[command(description = "set.language")]
    SetLang(LanguageCode),
}

pub type HandlerResult = Result<(), Box<dyn std::error::Error + Send + Sync>>;

static COORDS_REGEXP: Lazy<Regex> = Lazy::new(|| Regex::new(r"^(?P<latitude>-?\d{1,2}([.,]\d+)?),?\s+(?P<longitude>-?\d{1,3}([.,]\d+)?)$")
    .expect("Invalid coords regex!"));
static QUERY_REGEX: Lazy<Regex> = Lazy::new(|| Regex::new(r#"(\pL(\pM)?){3,}"#)
    .expect("Invalid query regex!"));
static FINDER: Lazy<SearchChain> = Lazy::new(|| {
    let osm = finder("OSM", osm::OpenStreetMapLocFinder::new());
    let yandex = finder("YANDEX", yandex::YandexLocFinder::from_env());
    let google = finder("GOOGLE", google::GoogleLocFinder::from_env());

    SearchChain::new(vec![
        google.clone(),
        osm.clone(),
        yandex.clone(),
    ]).for_lang_code("ru", vec![
        yandex,
        google,
        osm,
    ])
});
static INLINE_REQUESTS_LIMITER: Lazy<RequestsLimiter> = Lazy::new(|| RequestsLimiter::from_env(&REDIS.pool));

pub fn preload_env_vars() {
    google::preload_env_vars();
    yandex::preload_env_vars();

    query::preload_env_vars();

    let _ = *COORDS_REGEXP;
    let _ = *QUERY_REGEX;
    let _ = *FINDER;
    let _ = *INLINE_REQUESTS_LIMITER;
}

pub async fn inline_handler(bot: Bot, q: InlineQuery, usr_client: UserService<UserServiceClientGrpc>) -> HandlerResult {
    if !is_query_correct(&q.query) || rate_limit_exceeded(&q).await {
        bot.answer_inline_query(q.id, vec![]).await?;
        return Ok(());
    }

    log::info!("Got an inline query: {}", q.query);
    metrics::INLINE_COUNTER.inc_allowed();

    let lang_code = &ensure_lang_code(q.from.id, q.from.language_code.clone(), &usr_client).await;
    let location = try_determine_location(q.from.id, &usr_client).await;
    let locations = resolve_locations(q.query, lang_code, location).await?;

    senders::send_locations_inline(bot, q.id, lang_code, locations).await
}

async fn rate_limit_exceeded(q: &InlineQuery) -> bool {
    let forbidden = !INLINE_REQUESTS_LIMITER.is_req_allowed(q).await;
    if forbidden {
        log::info!("Requests limit was exceeded for {}", q.from.id);
        metrics::INLINE_COUNTER.inc_forbidden();
    }
    forbidden
}

pub async fn inline_chosen_handler(_: Bot, _: ChosenInlineResult) -> HandlerResult {
    metrics::INLINE_CHOSEN_COUNTER.inc();
    Ok(())
}

fn is_query_correct(query: &str) -> bool {
    query.is_empty().not() && (
        QUERY_REGEX.is_match(query)   ||
        COORDS_REGEXP.is_match(query) ||
        *QUERY_CHECK_MODE != QueryCheckMode::Regex
    )
}

#[derive(From)]
enum AnswerMessage {
    Text(String),
    TextWithMarkup(String, ReplyMarkup),
}

pub async fn command_handler(bot: Bot, msg: Message, cmd: Command, me: Me, usr_client: UserService<UserServiceClientGrpc>) -> HandlerResult {
    let help_or_status: AnswerMessage = match cmd {
        Command::Start if msg.from.is_some() => {
            metrics::CMD_START_COUNTER.inc();
            help::get_start_message(msg.from.as_ref().unwrap(), me, usr_client).await.into()
        },
        Command::Start => {
            log::warn!("The /start command was invoked without a FROM field for a message: {msg:?}");
            let lang_code = &determine_lang_code(&msg, &usr_client).await?;
            help::get_help_message(me, lang_code).into()
        }
        Command::Help => {
            metrics::CMD_HELP_COUNTER.inc();
            let lang_code = &determine_lang_code(&msg, &usr_client).await?;
            help::get_help_message(me, lang_code).into()
        }
        Command::Loc => {
            metrics::CMD_LOC_COUNTER.inc();
            // return from the outer function
            return cmd_loc_handler(bot, msg, usr_client).await
        }
        Command::SetLanguage(code) | Command::SetLang(code) if msg.from.is_some() && usr_client.enabled() => {
            metrics::CMD_SET_LANGUAGE_COUNTER.inc();
            let user = msg.from.as_ref().unwrap();
            options::cmd_set_language_handler(usr_client.unwrap(), user, code).await?
        }
        _ if usr_client.disabled() => {
            let lang_code = &determine_lang_code(&msg, &usr_client).await?;
            log::error!("user-service is disabled but a command was invoked by {:?}", msg.from);
            t!("error.service.user.disabled", locale = lang_code).to_string().into()
        },
        _ if msg.from.is_none() => Err(anyhow!("some command was invoked without a FROM field for a message: {msg:?}"))?,
        _ => Err(anyhow!("unexpected match arm in the command_handler"))?
    };
    process_answer_message(bot, msg.chat.id, help_or_status).await?;
    Ok(())
}

pub async fn message_handler(bot: Bot, msg: Message, usr_client: UserService<UserServiceClientGrpc>) -> HandlerResult {
    if !msg.chat.is_private() {
        return Ok(())
    }

    metrics::MESSAGE_COUNTER.inc();
    cmd_loc_handler(bot, msg, usr_client).await
}

pub async fn callback_handler(bot: Bot, q: CallbackQuery) -> HandlerResult {
    log::info!("Got a callback query for {}: {}",
        q.from.id,
        q.data.clone().unwrap_or("<null>".to_string()));

    let mut answer = bot.answer_callback_query(q.clone().id);
    if let (Some(chat_id), Some(data)) = (q.chat_id(), q.data) {
        let parts: Vec<&str> = data.split(',').collect();
        if parts.len() != 2 {
            Err("unexpected format of callback data")?;
        }
        let latitude: f64 = parts.first().unwrap().parse()?;
        let longitude: f64 = parts.get(1).unwrap().parse()?;
        bot.send_location(chat_id, latitude, longitude).await?;
    } else {
        let lang_code = q.from.language_code.unwrap_or_default();
        answer.text = Some(t!("error.old-message", locale = &lang_code).to_string());
        answer.show_alert = Some(true);
    }
    answer.await?;
    Ok(())
}

async fn cmd_loc_handler(bot: Bot, msg: Message, usr_client: UserService<impl UserServiceClient>) -> HandlerResult {
    let from = msg.from.as_ref().ok_or("no from")?;
    let lang_code = &ensure_lang_code(from.id, from.language_code.clone(), &usr_client).await;

    let text = match msg.text() {
        None => return send_error(bot, msg, "error.query.empty", lang_code).await,
        Some(text) => text.to_string()
    };
    log::info!("Got a message query: {}", text);

    let location = try_determine_location(from.id, &usr_client).await;
    let locations = resolve_locations(text, lang_code, location).await?;
    senders::send_locations_as_messages(bot, msg.chat.id, locations, lang_code).await?;
    Ok(())
}

async fn resolve_locations(query: String, lang_code: &str, location: Option<(f64, f64)>) -> Result<Vec<Location>, Box<dyn std::error::Error + Send + Sync>> {
    let query = query.as_str();
    let locations = if let Some(coords) = COORDS_REGEXP.captures(query) {
        let lat: f64 = coords["latitude"].parse()?;
        let long: f64 = coords["longitude"].parse()?;
        vec![Location::new(lat, long)]
    } else {
        FINDER.find(query, lang_code, location).await
    };
    Ok(locations)
}

async fn determine_lang_code(msg: &Message, usr_client: &UserService<impl UserServiceClient>) -> anyhow::Result<String> {
    let from = msg.from.as_ref().ok_or(anyhow!("no from"))?;
    Ok(ensure_lang_code(from.id, from.language_code.clone(), usr_client).await)
}

async fn process_answer_message(bot: Bot, chat_id: ChatId, answer: AnswerMessage) -> HandlerResult {
    let (text, keyboard) = match answer {
        AnswerMessage::Text(text) => (text, None),
        AnswerMessage::TextWithMarkup(text, keyboard) => (text, Some(keyboard))
    };

    let mut req = bot.send_message(chat_id, text)
        .parse_mode(Html);
    req.reply_markup = keyboard;
    req.await?;
    Ok(())
}

async fn send_error(bot: Bot, msg: Message, error_key: &str, lang_code: &str) -> HandlerResult {
    bot.send_message(msg.chat.id, t!(error_key, locale = lang_code))
        .parse_mode(MarkdownV2)
        .await
        .map(|_| ())
        .map_err(Into::into)
}
