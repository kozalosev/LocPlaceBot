use derive_more::From;
use rust_i18n::t;
use teloxide::Bot;
use teloxide::dispatching::dialogue::InMemStorage;
use teloxide::macros::BotCommands;
use teloxide::payloads::{SendMessageSetters};
use teloxide::prelude::Dialogue;
use teloxide::requests::Requester;
use teloxide::types::{ButtonRequest, InlineKeyboardButton, InlineKeyboardMarkup, KeyboardButton, KeyboardMarkup, KeyboardRemove, Message, ReplyMarkup, User};
use teloxide::types::ParseMode::MarkdownV2;
use crate::handlers::{AnswerMessage, HandlerResult, process_answer_message};
use crate::handlers::options::callback::CancellationCallbackData;
use crate::handlers::options::register_user;
use crate::metrics;
use crate::users::{UserService, UserServiceClient, UserServiceClientGrpc};
use crate::utils::ensure_lang_code;

#[derive(BotCommands, Clone)]
#[command(rename_rule = "lowercase")]
pub enum Commands {
    SetLocation,
    SetLoc,
}

#[derive(Clone, Default)]
pub enum LocationState {
    #[default]
    Start,
    Requested,
}

pub(super) type LocationDialogue = Dialogue<LocationState, InMemStorage<LocationState>>;

#[derive(From)]
enum MaybeContext<USC: UserServiceClient> {
    DialogueContext { usr_client: USC, lang_code: String },
    MessageToSend(AnswerMessage),
}

pub async fn start(bot: Bot, dialogue: LocationDialogue, msg: Message, usr_client: UserService<UserServiceClientGrpc>) -> HandlerResult {
    metrics::CMD_SET_LOCATION_COUNTER.invoked();
    let user = msg.from().ok_or("no user")?;

    let lang_code = match build_context(user, usr_client).await? {
        MaybeContext::DialogueContext { lang_code, .. } => lang_code,
        MaybeContext::MessageToSend(answer) => return process_answer_message(bot, msg.chat.id, answer).await
    };

    let msg_text = t!("set-option.location.message.text", locale = &lang_code);
    let btn_text = t!("set-option.location.message.button", locale = &lang_code);
    let keyboard = KeyboardMarkup::new(vec![vec![
        KeyboardButton::new(btn_text).request(ButtonRequest::Location)
    ]]);

    bot.send_message(msg.chat.id, msg_text)
        .parse_mode(MarkdownV2)
        .reply_markup(keyboard)
        .await?;

    dialogue.update(LocationState::Requested).await?;
    Ok(())
}

pub async fn requested(bot: Bot, msg: Message, dialogue: LocationDialogue, usr_client: UserService<UserServiceClientGrpc>) -> HandlerResult {
    let user = msg.from().ok_or("no user")?;
    let (client, lang_code) = match build_context(user, usr_client).await? {
        MaybeContext::DialogueContext { usr_client, lang_code } => (usr_client, lang_code),
        MaybeContext::MessageToSend(answer) => return process_answer_message(bot, msg.chat.id, answer).await
    };

    let location = match msg.location() {
        None => {
            let btn_text = t!("dialogue.cancel.button", locale = &lang_code);
            let btn_data = CancellationCallbackData::new(user.id);
            let cancellation_keyboard = InlineKeyboardMarkup::new(vec![vec![
                InlineKeyboardButton::callback(btn_text, btn_data.to_string())
            ]]);
            bot.send_message(msg.chat.id, t!("set-option.location.message.text", locale = &lang_code))
                .reply_markup(ReplyMarkup::InlineKeyboard(cancellation_keyboard))
                .await?;
            return dialogue.update(LocationState::Requested).await.map_err(Into::into)
        },
        Some(loc) => {
            dialogue.exit().await?;
            loc
        }
    };

    client.set_location(user.id, location.latitude, location.longitude).await?;
    metrics::CMD_SET_LOCATION_COUNTER.finished();

    let success_text = t!("set-option.location.success", locale = &lang_code);
    bot.send_message(msg.chat.id, success_text)
        .reply_markup(ReplyMarkup::KeyboardRemove(KeyboardRemove::default()))
        .await?;
    Ok(())
}

async fn build_context<USC: UserServiceClient>(user: &User, usr_client: UserService<USC>) -> anyhow::Result<MaybeContext<USC>> {
    use MaybeContext::*;

    let lang_code = ensure_lang_code(user.id, user.language_code.clone(), &usr_client.clone().into()).await;
    let res = match usr_client {
        UserService::Connected(client) => {
            if client.get(user.id).await?.is_none() {
                MessageToSend(register_user(client, user).await?)
            } else {
                DialogueContext { usr_client: client, lang_code }
            }
        }
        UserService::Disabled => AnswerMessage::Text(t!("error.service.user.disabled", locale = &lang_code)).into()
    };
    Ok(res)
}
