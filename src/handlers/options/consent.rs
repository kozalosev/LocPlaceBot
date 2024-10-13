use std::str::FromStr;
use std::sync::Arc;
use anyhow::anyhow;
use derive_more::{Constructor, Display};
use rust_i18n::t;
use teloxide::Bot;
use teloxide::dispatching::dialogue::GetChatId;
use teloxide::payloads::{AnswerCallbackQuerySetters, EditMessageTextSetters};
use teloxide::prelude::{CallbackQuery, UserId};
use teloxide::requests::Requester;
use teloxide::types::MaybeInaccessibleMessage;
use teloxide::types::ParseMode::Html;
use thiserror::Error;
use crate::{eula, CommandCacheStorage};
use crate::handlers::HandlerResult;
use crate::handlers::options::build_agreement_text;
use crate::handlers::options::callback::{CallbackHandlerDIParams, CallbackPreprocessorResult, preprocess_callback, UserIdAware};
use crate::handlers::options::location::{LocationDialogue, send_location_request};
use crate::users::{Consent, UserService, UserServiceClient, UserServiceClientGrpc};
use crate::utils::get_full_name;

#[derive(Display, Constructor)]
#[display("consent:{uid}:{lang_code}:{command}")]
pub struct ConsentCallbackData {
    uid: UserId,
    lang_code: String,
    command: SavedSetCommand,
}

#[derive(Debug, Display, Error)]
pub struct InvalidConsentCallbackData(String);

impl TryFrom<String> for ConsentCallbackData {
    type Error = InvalidConsentCallbackData;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        let parts: Vec<&str> = value.splitn(4, ':').collect();
        if parts.len() != 4 || parts[0] != "consent" {
            return Err(InvalidConsentCallbackData(value))
        }
        let uid = parts[1].parse()
            .map_err(|_| InvalidConsentCallbackData(value.clone()))?;
        Ok(Self {
            uid: UserId(uid),
            lang_code: parts[2].to_owned(),
            command: SavedSetCommand::from_str(parts[3])
                .map_err(|_| InvalidConsentCallbackData(value))?
        })
    }
}

#[derive(Display)]
pub enum SavedSetCommand {
    #[display("loc")]
    Location,
    #[display("lang:{_0}")]
    Language(String)
}

impl FromStr for SavedSetCommand {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.split_once(':') {
            Some(("lang", value)) => Ok(Self::Language(value.to_owned())),
            None if s == "loc" => Ok(Self::Location),
            _ => Err(())
        }
    }
}

impl UserIdAware for ConsentCallbackData {
    fn user_id(&self) -> UserId {
        self.uid
    }
}

pub fn callback_filter(query: CallbackQuery) -> bool {
    query.data
        .filter(|data| data.starts_with("consent:"))
        .is_some()
}

pub async fn callback_handler(bot: Bot, query: CallbackQuery, usr_client: UserService<UserServiceClientGrpc>,
                              dialogue_storage: Arc<CommandCacheStorage>) -> HandlerResult {
    let maybe_chat_id = query.chat_id();
    let data = ConsentCallbackData::try_from(query.data.clone().ok_or("no data")?)?;
    let ctx = match preprocess_callback(CallbackHandlerDIParams::new(&bot, &query, usr_client), &data).await? {
        CallbackPreprocessorResult::Processed(context) => context,
        CallbackPreprocessorResult::ErrorSent => return Ok(())
    };

    match query.message {
        Some(MaybeInaccessibleMessage::Regular(msg)) if msg.text().is_some() => {
            let eula_hash = eula::get_in(&data.lang_code).hash;
            let consent = Consent::new(msg.id, eula_hash);
            let name = get_full_name(&query.from);
            ctx.usr_client.register(query.from.id, name.clone(), consent).await?;

            let name = teloxide::utils::html::escape(&name);
            let new_text = format!("{}\n\n{}", build_agreement_text(&ctx.lang_code),
                                   t!("registration.consent.appendix", locale = &ctx.lang_code, username = name));
            bot.edit_message_text(msg.chat.id, msg.id, new_text)
                .parse_mode(Html)
                .await?;
            ctx.answer.show_alert(false)
                .text(t!("registration.consent.ok", locale = &ctx.lang_code))
        },
        Some(MaybeInaccessibleMessage::Inaccessible(_)) => ctx.answer.show_alert(true)
            .text(t!("error.old-message", locale = &ctx.lang_code)),
        Some(MaybeInaccessibleMessage::Regular(_))      => Err(anyhow!("no text of the message"))?,
        None                                            => Err(anyhow!("no message in the callback query"))?,
    }.await?;

    let chat_id = maybe_chat_id.ok_or("no chat_id")?;
    match data.command {
        SavedSetCommand::Language(code) => {
            ctx.usr_client.set_language(query.from.id, &code).await?;
            let text = t!("set-option.language.success", locale = &code);
            bot.send_message(chat_id, text).await?;
        }
        SavedSetCommand::Location => {
            let dialogue = LocationDialogue::new(dialogue_storage, chat_id);
            send_location_request(bot, chat_id, dialogue, &ctx.lang_code).await?;
        }
    };

    Ok(())
}
