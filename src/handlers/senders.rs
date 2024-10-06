use once_cell::sync::Lazy;
use rust_i18n::t;
use teloxide::prelude::*;
use teloxide::RequestError;
use teloxide::types::{InlineKeyboardButton, InlineKeyboardMarkup, InlineQueryResult, InlineQueryResultLocation};
use teloxide::types::ReplyMarkup::InlineKeyboard;
use super::HandlerResult;
use crate::loc::Location;

static CACHE_TIME: Lazy<Option<u32>> = Lazy::new(|| std::env::var("CACHE_TIME")
    .ok()
    .and_then(|v| { v.parse().ok() })
);
static MSG_LOC_LIMIT: Lazy<usize> = Lazy::new(|| std::env::var("MSG_LOC_LIMIT")
    .ok()
    .and_then(|v| { v.parse().ok() })
    .unwrap_or(10)
);

pub async fn send_locations_inline(bot: Bot, query_id: String, lang_code: &str, locations: Vec<Location>) -> HandlerResult {
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

pub async fn send_locations_as_messages(bot: Bot, chat_id: ChatId, locations: Vec<Location>, lang_code: &str) -> Result<Message, RequestError> {
    match locations.len() {
        0 => bot.send_message(chat_id, t!("title.address-list.empty", locale = lang_code)).await,
        1 => send_single_location(&bot, chat_id, locations.first().unwrap()).await,
        _ => send_locations_keyboard(&bot, chat_id, locations, lang_code).await
    }
}

async fn send_locations_keyboard(bot: &Bot, chat_id: ChatId, locations: Vec<Location>, lang_code: &str) -> Result<Message, RequestError> {
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

    log::debug!("Send locations keyboard for {}: {:?}", chat_id, *msg);
    msg.await
}

async fn send_single_location(bot: &Bot, chat_id: ChatId, location: &Location) -> Result<Message, RequestError> {
    if let Some(addr) = location.address() {
        bot.send_message(chat_id, addr).await?;
    }
    bot.send_location(chat_id, location.latitude(), location.longitude()).await
}