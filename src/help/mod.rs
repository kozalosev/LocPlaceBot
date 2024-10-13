use rust_i18n::t;
use teloxide::types::{Me, User};
use crate::users::{UserService, UserServiceClient};
use crate::utils::ensure_lang_code;

static EN_HELP: &str = include_str!("en.html");
static RU_HELP: &str = include_str!("ru.html");

pub async fn get_start_message(from: &User, me: Me, usr_client: UserService<impl UserServiceClient>) -> String {
    let lang_code = &ensure_lang_code(from.id, from.language_code.clone(), &usr_client).await;
    let greeting = t!("title.greeting", locale = lang_code);
    let name = teloxide::utils::html::escape(&from.first_name);
    format!("{}, <b>{}</b>!\n\n{}", greeting, name, get_help_message(me, lang_code))
}

pub fn get_help_message(me: Me, lang_code: &str) -> String {
    let help_template = match lang_code {
        "ru" => RU_HELP,
        _    => EN_HELP
    };
    help_template.replace("{{bot_name}}", me.username())
}