mod senders;

use regex::Regex;
use once_cell::sync::Lazy;
use rust_i18n::t;
use crate::help;
use crate::loc::{Location, SearchChain, google, yandex, osm};
use crate::metrics::{MESSAGE_COUNTER, INLINE_COUNTER, INLINE_CHOSEN_COUNTER, CMD_HELP_COUNTER, CMD_START_COUNTER, CMD_LOC_COUNTER};
use crate::utils::ensure_lang_code;
use teloxide::prelude::*;
use teloxide::dispatching::dialogue::GetChatId;
use teloxide::types::Me;
use teloxide::types::ParseMode::MarkdownV2;
use teloxide::utils::command::BotCommands;

#[derive(BotCommands, Clone)]
#[command(rename_rule = "lowercase")]
pub enum Command {
    Help,
    Start,
    Loc,
}

pub type HandlerResult = Result<(), Box<dyn std::error::Error + Send + Sync>>;

static COORDS_REGEXP: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?P<latitude>-?\d{1,2}(\.\d+)?),? (?P<longitude>-?\d{1,3}(\.\d+)?)")
    .expect("Invalid regex!"));
static FINDER: Lazy<SearchChain> = Lazy::new(|| {
        SearchChain::new(vec![
            Box::new(osm::OpenStreetMapLocFinder::new()),
            Box::new(google::GoogleLocFinder::from_env())
        ]).for_lang_code("ru", vec![
            Box::new(yandex::YandexLocFinder::from_env())
        ])
});

pub fn preload_env_vars() {
    google::preload_env_vars();
    yandex::preload_env_vars();
}

pub async fn inline_handler(bot: Bot, q: InlineQuery) -> HandlerResult {
    if q.query.is_empty() {
        bot.answer_inline_query(q.id, vec![]).await?;
        return Ok(());
    }

    log::info!("Got an inline query: {}", q.query);
    INLINE_COUNTER.inc();

    let lang_code = &ensure_lang_code(q.from.id, q.from.language_code.clone());
    let locations = resolve_locations(q.query, lang_code).await?;

    senders::send_locations_inline(bot, q.id, lang_code, locations).await
}

pub async fn inline_chosen_handler(_: Bot, _: ChosenInlineResult) -> HandlerResult {
    INLINE_CHOSEN_COUNTER.inc();
    Ok(())
}

pub async fn command_handler(bot: Bot, msg: Message, cmd: Command, me: Me) -> HandlerResult {
    let help = match cmd {
        Command::Start if msg.from().is_some() => {
            CMD_START_COUNTER.inc();
            help::get_start_message(msg.from().unwrap(), me)
        },
        Command::Start => {
            log::warn!("The /start command was invoked without a FROM field for message: {:?}", msg);
            help::get_help_message(msg.from(), me)
        }
        Command::Help => {
            CMD_HELP_COUNTER.inc();
            help::get_help_message(msg.from(), me)
        },
        Command::Loc => {
            CMD_LOC_COUNTER.inc();
            // return from the outer function
            return cmd_loc_handler(bot, msg).await
        }
    };

    let mut answer = bot.send_message(msg.chat.id, help);
    answer.parse_mode = Some(MarkdownV2);
    answer.await?;
    Ok(())
}

pub async fn message_handler(bot: Bot, msg: Message) -> HandlerResult {
    if !msg.chat.is_private() {
        return Ok(())
    }

    MESSAGE_COUNTER.inc();
    cmd_loc_handler(bot, msg).await
}

pub async fn callback_handler(bot: Bot, q: CallbackQuery) -> HandlerResult {
    log::info!("Got a callback query for {}: {}",
        q.from.id,
        q.data.clone().unwrap_or("<null>".to_string()));

    let mut answer = bot.answer_callback_query(q.clone().id);
    if let (Some(chat_id), Some(data)) = (q.chat_id(), q.data) {
        let parts: Vec<&str> = data.split(",").collect();
        if parts.len() != 2 {
            Err("unexpected format of callback data")?;
        }
        let latitude: f64 = parts.get(0).unwrap().parse()?;
        let longitude: f64 = parts.get(1).unwrap().parse()?;
        bot.send_location(chat_id, latitude, longitude).await?;
    } else {
        let lang_code = q.from.language_code.unwrap_or(String::default());
        answer.text = Some(t!("error.old-message", locale = lang_code.as_str()));
        answer.show_alert = Some(true);
    }
    answer.await?;
    Ok(())
}

async fn cmd_loc_handler(bot: Bot, msg: Message) -> HandlerResult {
    let locations = resolve_locations_for_message(&msg).await?;
    let lang_code = msg.from()
        .and_then(|u| u.language_code.clone())
        .unwrap_or(String::default());
    senders::send_locations_as_messages(bot, msg.chat.id, locations, lang_code.as_str()).await?;
    Ok(())
}

async fn resolve_locations_for_message(msg: &Message) -> Result<Vec<Location>, Box<dyn std::error::Error + Send + Sync>> {
    let text = msg.text().ok_or("no text")?.to_string();
    let from = msg.from().ok_or("no from")?;
    log::info!("Got a message query: {}", text);

    let lang_code = &ensure_lang_code(from.id, from.language_code.clone());
    resolve_locations(text, lang_code).await
}

async fn resolve_locations(query: String, lang_code: &str) -> Result<Vec<Location>, Box<dyn std::error::Error + Send + Sync>> {
    let query = query.as_str();
    let locations = if let Some(coords) = COORDS_REGEXP.captures(query) {
        let lat: f64 = coords["latitude"].parse()?;
        let long: f64 = coords["longitude"].parse()?;
        vec![Location::new(lat, long)]
    } else {
        FINDER.find(query, lang_code).await
    };
    Ok(locations)
}
