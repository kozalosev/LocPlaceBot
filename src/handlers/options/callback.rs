use derive_more::Constructor;
use rust_i18n::t;
use teloxide::Bot;
use teloxide::payloads::{AnswerCallbackQuery, AnswerCallbackQuerySetters};
use teloxide::prelude::{CallbackQuery, Requester, UserId};
use teloxide::requests::JsonRequest;
use crate::users::{UserService, UserServiceClient};
use crate::utils::ensure_lang_code;

pub(super) trait UserIdAware {
    fn user_id(&self) -> UserId;
}

#[derive(Constructor)]
pub(super) struct CallbackHandlerDIParams<'a, USC: UserServiceClient> {
    bot: &'a Bot,
    query: &'a CallbackQuery,
    usr_client: UserService<USC>,
}

pub(super) struct CallbackContext<USC: UserServiceClient> {
    pub lang_code: String,
    pub answer: JsonRequest<AnswerCallbackQuery>,
    pub usr_client: USC,
}

pub(super) enum CallbackPreprocessorResult<USC: UserServiceClient> {
    Processed(CallbackContext<USC>),
    ErrorSent,
}

pub(super) async fn preprocess_callback<USC, D>(p: CallbackHandlerDIParams<'_, USC>, data: &D) -> anyhow::Result<CallbackPreprocessorResult<USC>>
    where
        USC: UserServiceClient,
        D: UserIdAware
{
    use CallbackPreprocessorResult::*;

    let lang_code = ensure_lang_code(p.query.from.id, p.query.from.language_code.clone(), &p.usr_client).await;
    let answer = p.bot.answer_callback_query(p.query.id.clone());

    if data.user_id() != p.query.from.id {
        answer.show_alert(true)
            .text(t!("error.callbacks.another-person", locale = &lang_code))
            .await?;
        return Ok(ErrorSent)
    }

    let usr_client = match p.usr_client {
        UserService::Connected(client) => client,
        UserService::Disabled => {
            answer.show_alert(true)
                .text(t!("error.service.user.disabled", locale = &lang_code))
                .await?;
            return Ok(ErrorSent)
        }
    };

    Ok(Processed(CallbackContext {
        lang_code,
        answer,
        usr_client,
    }))
}
