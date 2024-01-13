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

#[derive(Debug, strum_macros::Display, Clone)]
#[strum(serialize_all="lowercase")]
pub enum LanguageCode {
    Ru,
    En { fallback_for: Option<String> },
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
            _ => Ok(Self::En { fallback_for: Some(code) })
        }
    }
}

pub(super) async fn cmd_set_language_handler(usr_client: impl UserServiceClient, user: &teloxide::types::User, code: LanguageCode) -> anyhow::Result<AnswerMessage> {
    if let LanguageCode::En { fallback_for } = &code {
        if fallback_for.is_some() {
            let requested_code = fallback_for.as_ref().unwrap();
            log::warn!("unsupported language was requested: {}", requested_code);
            let lang_code = &ensure_lang_code(user.id, None, &usr_client.into()).await;
            return Ok(t!("set-option.language.unsupported", locale = lang_code).into())
        }
    }

    let code = code.to_string();
    match usr_client.get(user.id).await? {
        Some(_) => {
            usr_client.set_language(user.id, &code).await?;
            Ok(t!("set-option.language.success", locale = &code).into())
        },
        None => register_user(usr_client, user).await
    }
}

async fn register_user(client: impl UserServiceClient, user: &teloxide::types::User) -> anyhow::Result<AnswerMessage> {
    let lang_code = &ensure_lang_code(user.id, user.language_code.clone(), &client.into()).await;

    let msg_text = build_agreement_text(lang_code);
    let btn_text = t!("registration.message.button", locale = lang_code);
    let btn_data = format!("consent:{}:{lang_code}", user.id);

    let keyboard = InlineKeyboardMarkup::new(vec![vec![
        InlineKeyboardButton::callback(btn_text, btn_data)
    ]]);
    Ok(AnswerMessage::TextWithMarkup(msg_text, keyboard.into()))
}

fn build_agreement_text(lang_code: &str) -> String {
    let agreement = eula::get_in(lang_code).text.to_owned();
    format!("{}\n\n{agreement}", t!("registration.message.text", locale = lang_code))
}
