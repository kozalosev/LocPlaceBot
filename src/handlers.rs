use once_cell::sync::Lazy;
use regex::Regex;
use rust_i18n::t;
use teloxide::dispatching::dialogue::GetChatId;
use teloxide::prelude::*;
use teloxide::RequestError;
use teloxide::types::{InlineKeyboardButton, InlineKeyboardMarkup, InlineQueryResult, InlineQueryResultLocation, Me, User};
use teloxide::types::ParseMode::MarkdownV2;
use teloxide::types::ReplyMarkup::InlineKeyboard;
use teloxide::utils::command::BotCommands;
use crate::config;
use crate::loc::{Location, LocFinder};
use crate::metrics::{MESSAGE_COUNTER, INLINE_COUNTER, INLINE_CHOSEN_COUNTER, CMD_HELP_COUNTER, CMD_START_COUNTER, CMD_LOC_COUNTER};

type HandlerResult = Result<(), Box<dyn std::error::Error + Send + Sync>>;

static CACHE_TIME: Lazy<Option<u32>> = Lazy::new(|| std::env::var("CACHE_TIME")
    .ok()
    .map(|v| { v.parse().ok() })
    .flatten()
);
static MSG_LOC_LIMIT: Lazy<usize> = Lazy::new(|| std::env::var("MSG_LOC_LIMIT")
    .ok()
    .map(|v| { v.parse().ok() })
    .flatten()
    .unwrap_or(10)
);

static COORDS_REGEXP: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?P<latitude>-?\d{1,2}(\.\d+)?),? (?P<longitude>-?\d{1,3}(\.\d+)?)")
    .expect("Invalid regex!"));
static FINDER: Lazy<LocFinder> = Lazy::new(|| LocFinder::from_env());

static EN_HELP: &str = include_str!("help/en.md");
static RU_HELP: &str = include_str!("help/ru.md");

pub async fn inline_handler(bot: Bot, q: InlineQuery) -> HandlerResult {
    if q.query.is_empty() {
        bot.answer_inline_query(q.id, vec![]).await?;
        return Ok(());
    }

    log::info!("Got query: {}", q.query);
    INLINE_COUNTER.inc();

    let lang_code = &ensure_lang_code(q.from.id, q.from.language_code.clone());
    let locations = resolve_locations(q.query, lang_code).await?;

    send_locations_inline(bot, q.id, lang_code, locations).await
}

pub async fn inline_chosen_handler(_: Bot, _: ChosenInlineResult) -> HandlerResult {
    INLINE_CHOSEN_COUNTER.inc();
    Ok(())
}

pub async fn command_handler(bot: Bot, msg: Message, cmd: Command, me: Me) -> HandlerResult {
    let help = match cmd {
        Command::Start if msg.from().is_some() => {
            CMD_START_COUNTER.inc();
            get_start_message(msg.from().unwrap(), me)
        },
        Command::Start => {
            log::warn!("The /start command was invoked without a FROM field for message: {:?}", msg);
            get_help_message(msg.from(), me)
        }
        Command::Help => {
            CMD_HELP_COUNTER.inc();
            get_help_message(msg.from(), me)
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
    send_locations_as_messages(bot, msg.chat.id, locations, lang_code.as_str()).await?;
    Ok(())
}

fn ensure_lang_code(uid: UserId, lang_code: Option<String>) -> String {
    lang_code
        .unwrap_or_else(|| {
            log::warn!("no language_code for {}, using the default", uid);
            String::default()
        })
}

async fn resolve_locations(query: String, lang_code: &str) -> Result<Vec<Location>, Box<dyn std::error::Error + Send + Sync>> {
    let query = query.as_str();
    let locations = if let Some(coords) = COORDS_REGEXP.captures(query) {
        let lat: f64 = coords["latitude"].parse()?;
        let long: f64 = coords["longitude"].parse()?;
        vec![Location::new(lat, long)]
    } else {
        match *config::GAPI_MODE {
            config::GoogleAPIMode::Place => FINDER.find_place(query, lang_code).await?,
            config::GoogleAPIMode::Text => FINDER.find_text(query, lang_code).await?,
            config::GoogleAPIMode::GeoPlace => FINDER.find(query, lang_code).await?,
            config::GoogleAPIMode::GeoText => FINDER.find_more(query, lang_code).await?,
        }
    };
    Ok(locations)
}

async fn resolve_locations_for_message(msg: &Message) -> Result<Vec<Location>, Box<dyn std::error::Error + Send + Sync>> {
    let text = msg.text().ok_or("no text")?.to_string();
    let from = msg.from().ok_or("no from")?;

    let lang_code = &ensure_lang_code(from.id, from.language_code.clone());
    resolve_locations(text, lang_code).await
}

async fn send_locations_inline(bot: Bot, query_id: String, lang_code: &str, locations: Vec<Location>) -> HandlerResult {
    let results: Vec<InlineQueryResult> = locations.iter()
        .map(|l| {
            let uuid = uuid::Uuid::new_v4().to_string();
            let address = l.address().unwrap_or(t!("title.address.point", locale = lang_code));
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

async fn send_locations_as_messages(bot: Bot, chat_id: ChatId, locations: Vec<Location>, lang_code: &str) -> Result<Message, RequestError> {
    match locations.len() {
        0 => bot.send_message(chat_id, t!("title.address-list.empty", locale = lang_code)).await,
        1 => send_single_location(bot, chat_id, locations.get(0).unwrap()).await,
        _ => send_locations_keyboard(bot, chat_id, locations, lang_code).await
    }
}

async fn send_locations_keyboard(bot: Bot, chat_id: ChatId, locations: Vec<Location>, lang_code: &str) -> Result<Message, RequestError> {
    let buttons: Vec<Vec<InlineKeyboardButton>> = locations.iter()
        .filter(|l| l.address().is_some())
        .take(*MSG_LOC_LIMIT)
        .map(|loc| {
            let addr = loc.address().unwrap();
            let data = format!("{},{}", loc.latitude(), loc.longitude());
            let btn = InlineKeyboardButton::callback(addr.clone(), data);
            vec!(btn)
        })
        .collect();

    let mut msg = bot.send_message(chat_id, t!("title.address-list.has-data", locale = lang_code));
    let keyboard = InlineKeyboardMarkup::new(buttons);
    msg.reply_markup = Some(InlineKeyboard(keyboard));
    msg.await
}

async fn send_single_location(bot: Bot, chat_id: ChatId, location: &Location) -> Result<Message, RequestError> {
    if let Some(addr) = location.address() {
        bot.send_message(chat_id, addr).await?;
    }
    bot.send_location(chat_id, location.latitude(), location.longitude()).await
}


#[derive(BotCommands, Clone)]
pub enum Command {
    Help,
    Start,
    Loc,
}

fn get_start_message(from: &User, me: Me) -> String {
    let lang_code = &ensure_lang_code(from.id, from.language_code.clone());
    let greeting = t!("title.greeting", locale = lang_code);
    format!("{}, *{}*\\!\n\n{}", greeting, from.first_name, get_help_message(Some(from), me))
}

fn get_help_message(from: Option<&User>, me: Me) -> String {
    let help_template = from.and_then(|u| u.language_code.clone())
        .filter(|lang_code| lang_code == "ru")
        .map(|_| RU_HELP)
        .unwrap_or(EN_HELP);
    help_template.replace("{{bot_name}}", me.username())
}
