use rust_i18n::t;
use teloxide::Bot;
use teloxide::payloads::{AnswerCallbackQuerySetters, EditMessageTextSetters};
use teloxide::prelude::{CallbackQuery, UserId};
use teloxide::requests::Requester;
use teloxide::types::ParseMode::MarkdownV2;
use crate::eula;
use crate::handlers::HandlerResult;
use crate::handlers::options::build_agreement_text;
use crate::handlers::options::callback::{CallbackHandlerDIParams, CallbackPreprocessorResult, preprocess_callback, UserIdAware};
use crate::users::{Consent, UserService, UserServiceClient, UserServiceClientGrpc};
use crate::utils::get_full_name;

struct ConsentCallbackData {
    uid: UserId,
    lang_code: String,
}

impl std::fmt::Display for ConsentCallbackData {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!("consent:{}:{}", self.uid, self.lang_code))
    }
}

#[derive(Debug, derive_more::Display, thiserror::Error)]
struct InvalidConsentCallbackData(String);

impl TryFrom<String> for ConsentCallbackData {
    type Error = InvalidConsentCallbackData;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        let parts: Vec<&str> = value.split(':').collect();
        if parts.len() != 3 || parts[0] != "consent" {
            return Err(InvalidConsentCallbackData(value))
        }
        let uid = parts[1].parse()
            .map_err(|_| InvalidConsentCallbackData(value.clone()))?;
        Ok(Self {
            uid: UserId(uid),
            lang_code: parts[2].to_owned(),
        })
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

pub async fn callback_handler(bot: Bot, query: CallbackQuery, usr_client: UserService<UserServiceClientGrpc>) -> HandlerResult {
    let data = ConsentCallbackData::try_from(query.data.clone().ok_or("no data")?)?;
    let ctx = match preprocess_callback(CallbackHandlerDIParams::new(&bot, &query, usr_client), &data).await? {
        CallbackPreprocessorResult::Processed(context) => context,
        CallbackPreprocessorResult::ErrorSent => return Ok(())
    };

    match query.message {
        Some(msg) if msg.text().is_some() => {
            let eula_hash = eula::get_in(&data.lang_code).hash;
            let consent = Consent::new(msg.id, eula_hash);
            let name = get_full_name(&query.from);
            ctx.usr_client.register(query.from.id, name.clone(), consent).await?;

            let name = teloxide::utils::markdown::escape(&name);
            let new_text = format!("{}\n\n{}", build_agreement_text(&ctx.lang_code),
                                   t!("registration.consent.appendix", locale = &ctx.lang_code, username = name));
            bot.edit_message_text(msg.chat.id, msg.id, new_text)
                .parse_mode(MarkdownV2)
                .await?;
            ctx.answer.show_alert(false)
                .text(t!("registration.consent.ok", locale = &ctx.lang_code))
        },
        _ => ctx.answer.show_alert(true)
            .text(t!("error.old-message", locale = &ctx.lang_code))
    }.await?;
    Ok(())
}
