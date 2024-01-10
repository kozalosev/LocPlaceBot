use std::str::FromStr;
use derive_more::{Constructor, Display, From};
use rust_i18n::t;
use teloxide::Bot;
use teloxide::payloads::{AnswerCallbackQuerySetters, SendMessageSetters};
use teloxide::prelude::{CallbackQuery, UserId};
use teloxide::requests::Requester;
use teloxide::types::{InlineKeyboardButton, InlineKeyboardMarkup, KeyboardRemove, Message, ReplyMarkup};
use teloxide::types::ParseMode::MarkdownV2;
use thiserror::Error;
use crate::handlers::{HandlerResult, process_answer_message};
use crate::handlers::options::callback::{CallbackHandlerDIParams, CallbackPreprocessorResult, preprocess_callback, UserIdAware};
use crate::handlers::options::register_user;
use crate::users::{UserService, UserServiceClient, UserServiceClientGrpc};
use crate::utils::ensure_lang_code;

#[derive(Debug, From)]
enum Opt {
    Location { latitude: f64, longitude: f64 }
}

#[derive(Debug, Display, Error, Clone)]
struct InvalidValue(String);

impl FromStr for Opt {
    type Err = InvalidValue;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let invalid = InvalidValue(s.to_owned());
        let (opt, params) = s.split_once(':').ok_or(invalid.clone())?;
        let params: Vec<&str> = params.split(':').collect();
        match (opt, params.len()) {
            ("location", 2) => {
                if let (Ok(latitude), Ok(longitude)) = (params[0].parse(), params[1].parse()) {
                    Ok(Self::Location { latitude, longitude })
                } else {
                    Err(invalid)
                }
            },
            _ => Err(invalid)
        }
    }
}

impl std::fmt::Display for Opt {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Opt::Location { latitude, longitude } =>
                f.write_fmt(format_args!("location:{latitude}:{longitude}"))
        }
    }
}

#[derive(Debug, Constructor)]
struct ConfirmationCallbackData {
    uid: UserId,
    option: Opt,
}

impl TryFrom<String> for ConfirmationCallbackData {
    type Error = InvalidValue;

    fn try_from(s: String) -> Result<Self, Self::Error> {
        s.strip_prefix("confirmation:")
            .and_then(|s1| s1.split_once(':'))
            .and_then(|(user_id, option)|
                if let (Ok(uid), Ok(opt)) = (user_id.parse(), Opt::from_str(option)) {
                    Some((uid, opt))
                } else {
                    None
                }
            )
            .map(|(uid, opt)| Self { uid: UserId(uid), option: opt })
            .ok_or(InvalidValue(s.to_owned()))
    }
}

impl std::fmt::Display for ConfirmationCallbackData {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!("confirmation:{}:{}", self.uid, self.option))
    }
}

impl UserIdAware for ConfirmationCallbackData {
    fn user_id(&self) -> UserId {
        self.uid
    }
}

impl ConfirmationCallbackData {
    fn check_prefix(s: &str) -> bool {
        s.starts_with("confirmation:")
    }
}

pub fn location_filter(msg: Message, usr_client: UserService<UserServiceClientGrpc>) -> bool {
    msg.location().is_some() && msg.from().is_some() && usr_client.enabled()
}

pub async fn location_handler(bot: Bot, msg: Message, usr_client: UserService<UserServiceClientGrpc>) -> HandlerResult {
    let from = msg.from().expect("from must be in the handler");
    let location = msg.location().expect("location must be in the handler");
    let lang_code = &ensure_lang_code(from.id, from.language_code.clone(), &usr_client).await;

    let client = usr_client.unwrap();
    if client.get(from.id).await?.is_none() {
        let answer = register_user(client, from).await?;
        return process_answer_message(bot, msg.chat.id, answer).await
    }

    let btn_text = t!("set-option.location.confirmation.button", locale = lang_code);
    let btn_data = ConfirmationCallbackData::new(from.id, Opt::from((location.latitude, location.longitude)));
    let confirmation_keyboard = InlineKeyboardMarkup::new(vec![vec![
        InlineKeyboardButton::callback(btn_text, btn_data.to_string())
    ]]);
    bot.send_message(msg.chat.id, t!("set-option.location.confirmation.text", locale = lang_code))
        .reply_markup(ReplyMarkup::InlineKeyboard(confirmation_keyboard))
        .await?;
    let service_msg = bot.send_message(msg.chat.id, t!("set-option.location.confirmation.remove-keyboard", locale = lang_code))
        .parse_mode(MarkdownV2)
        .reply_markup(ReplyMarkup::KeyboardRemove(KeyboardRemove::default()))
        .await?;
    bot.delete_message(msg.chat.id, service_msg.id).await?;
    Ok(())
}

pub fn callback_filter(query: CallbackQuery) -> bool {
    query.data
        .filter(|data| ConfirmationCallbackData::check_prefix(data))
        .is_some()
}

pub async fn callback_handler(bot: Bot, query: CallbackQuery, usr_client: UserService<UserServiceClientGrpc>) -> HandlerResult {
    let data = ConfirmationCallbackData::try_from(query.data.clone().ok_or("no data")?)?;
    let ctx = match preprocess_callback(CallbackHandlerDIParams::new(&bot, &query, usr_client), &data).await? {
        CallbackPreprocessorResult::Processed(context) => context,
        CallbackPreprocessorResult::ErrorSent => return Ok(())
    };

    match data.option {
        Opt::Location { latitude, longitude } => ctx.usr_client.set_location(query.from.id, latitude, longitude).await?
    }

    let success_text = t!("set-option.location.success", locale = &ctx.lang_code);
    if let Some(msg) = query.message {
        bot.edit_message_text(msg.chat.id, msg.id, success_text)
            .await?;
    } else {
        ctx.answer.show_alert(true)
            .text(success_text)
            .await?;
    }
    Ok(())
}
