pub mod consent;
pub mod location;
mod callback;

use std::convert::Infallible;
use std::str::FromStr;
use rust_i18n::t;
use teloxide::types::{InlineKeyboardButton, InlineKeyboardMarkup};
use crate::eula;
use crate::handlers::AnswerMessage;
use crate::users::UserServiceClient;
use crate::utils::ensure_lang_code;

pub use callback::{cancellation_filter, cancellation_handler, CancellationCallbackData};
use crate::handlers::options::consent::{ConsentCallbackData, SavedSetCommand};

#[derive(Debug, strum_macros::Display, Clone)]
#[strum(serialize_all="lowercase")]
pub enum LanguageCode {
    Ru,
    En { fallback_for: Option<String> },
    Empty,
}

impl FromStr for LanguageCode {
    type Err = Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let code = if s.len() > 2 {
            s.chars().take(2).collect()
        } else {
            s.to_owned()
        }.to_lowercase();
        match code.as_str() {
            "ru" | "be" | "uk" | "🇷🇺" | "🇺🇦" | "🇧🇾" => Ok(Self::Ru),
            "en" | "🇺🇸" | "🇬🇧" => Ok(Self::En { fallback_for: None }),
            "" => Ok(Self::Empty),
            _ => Ok(Self::En { fallback_for: Some(code) })
        }
    }
}

pub(super) async fn cmd_set_language_handler(usr_client: impl UserServiceClient, user: &teloxide::types::User, code: LanguageCode) -> anyhow::Result<AnswerMessage> {
    let answer = match &code {
        LanguageCode::En { fallback_for: Some(requested_code) } => {
            log::warn!("unsupported language was requested: {}", requested_code);
            let lang_code = &ensure_lang_code(user.id, None, &usr_client.into()).await;
            t!("set-option.language.unsupported", locale = lang_code).to_string().into()
        }
        LanguageCode::Empty => {
            let lang_code = &ensure_lang_code(user.id, None, &usr_client.into()).await;
            t!("set-option.language.empty", locale = lang_code).to_string().into()
        }
        code => {
            let code = code.to_string();
            match usr_client.get(user.id).await? {
                Some(_) => {
                    usr_client.set_language(user.id, &code).await?;
                    t!("set-option.language.success", locale = &code).to_string().into()
                },
                None => register_user(usr_client, user, SavedSetCommand::Language(code)).await?
            }
        }
    };
    Ok(answer)
}

async fn register_user(client: impl UserServiceClient, user: &teloxide::types::User, cmd: SavedSetCommand) -> anyhow::Result<AnswerMessage> {
    let lang_code = &ensure_lang_code(user.id, user.language_code.clone(), &client.into()).await;

    let msg_text = build_agreement_text(lang_code);
    let btn_text = t!("registration.message.button", locale = lang_code);
    let btn_data = ConsentCallbackData::new(user.id, lang_code.to_owned(), cmd);

    let keyboard = InlineKeyboardMarkup::new(vec![vec![
        InlineKeyboardButton::callback(btn_text, btn_data.to_string())
    ]]);
    Ok(AnswerMessage::TextWithMarkup(msg_text, keyboard.into()))
}

fn build_agreement_text(lang_code: &str) -> String {
    let agreement = eula::get_in(lang_code).text.to_owned();
    format!("{}\n\n{agreement}", t!("registration.message.text", locale = lang_code))
}
