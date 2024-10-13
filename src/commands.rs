use rust_i18n::t;
use teloxide::{Bot, RequestError};
use teloxide::payloads::SetMyCommandsSetters;
use teloxide::requests::Requester;
use teloxide::types::{BotCommand, BotCommandScope};
use teloxide::utils::command::BotCommands;
use crate::handlers;

pub async fn set_my_commands(bot: &Bot, lang_code: &str) -> Result<(), RequestError> {
    let commands = [
        handlers::Command::bot_commands(),
        handlers::options::location::Commands::bot_commands(),
    ];

    let commands: Vec<BotCommand> = commands
        .concat()
        .into_iter()
        .filter(|cmd| !cmd.description.is_empty())
        .map(|mut cmd| {
            cmd.description = t!(format!("cmd-description.{}", cmd.description), locale = lang_code).to_string();
            cmd
        })
        .collect();
    bot.set_my_commands(commands)
        .language_code(lang_code.to_owned())
        .scope(BotCommandScope::Default)
        .await?;
    Ok(())
}
