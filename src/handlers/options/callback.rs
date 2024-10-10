use std::str::FromStr;
use derive_more::{Constructor, Display};
use rust_i18n::t;
use teloxide::Bot;
use teloxide::dispatching::dialogue::InMemStorage;
use teloxide::payloads::{AnswerCallbackQuery, AnswerCallbackQuerySetters, SendMessageSetters};
use teloxide::prelude::{CallbackQuery, Dialogue, Requester, UserId};
use teloxide::requests::JsonRequest;
use teloxide::types::ParseMode::Html;
use teloxide::types::{KeyboardRemove, MaybeInaccessibleMessage, ReplyMarkup};
use crate::handlers::HandlerResult;
use crate::users::{UserService, UserServiceClient, UserServiceClientGrpc};
use crate::utils::ensure_lang_code;

pub trait UserIdAware {
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

#[derive(Constructor, Display)]
#[display("cancel:{uid}")]
pub struct CancellationCallbackData {
    uid: UserId
}

impl FromStr for CancellationCallbackData {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        s.strip_prefix("cancel:")
            .and_then(|suffix| suffix.parse().ok())
            .map(UserId)
            .map(Self::new)
            .ok_or(())
    }
}

impl UserIdAware for CancellationCallbackData {
    fn user_id(&self) -> UserId {
        self.uid
    }
}

pub fn cancellation_filter<CD: FromStr>(query: CallbackQuery) -> bool {
    query.data
        .filter(|v| CD::from_str(v).is_ok())
        .is_some()
}

pub async fn cancellation_handler<S, CD>(bot: Bot, dialogue: Dialogue<S, InMemStorage<S>>, query: CallbackQuery, usr_client: UserService<UserServiceClientGrpc>) -> HandlerResult
where
    S: Clone + Send + 'static,
    CD: UserIdAware + FromStr
{
    let data = query.data.as_ref()
        .and_then(|v| CD::from_str(v).ok())
        .ok_or("the callback data is missing unexpectedly")?;
    let ctx = match preprocess_callback(CallbackHandlerDIParams::new(&bot, &query, usr_client), &data).await? {
        CallbackPreprocessorResult::Processed(context) => context,
        CallbackPreprocessorResult::ErrorSent => return Ok(())
    };
    let maybe_chat_id = query.message.map(|msg| match msg {
        MaybeInaccessibleMessage::Regular(m)               => m.chat.id,
        MaybeInaccessibleMessage::Inaccessible(m) => m.chat.id,
    });

    if let Some(chat_id) = maybe_chat_id {
        let placeholder = t!("set-option.location.remove-keyboard", locale = &ctx.lang_code);
        let service_msg = bot.send_message(chat_id, placeholder)
            .parse_mode(Html)
            .reply_markup(ReplyMarkup::KeyboardRemove(KeyboardRemove::default()))
            .await?;
        bot.delete_message(chat_id, service_msg.id).await?;
    }
    dialogue.exit().await?;
    Ok(())
}
