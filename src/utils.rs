use teloxide::prelude::UserId;
use teloxide::types::User;
use crate::users::{UserService, UserServiceClient};

pub async fn ensure_lang_code(uid: UserId, lang_code: Option<String>, usr_srv_client: &UserService<impl UserServiceClient>) -> String {
    match usr_srv_client {
        UserService::Connected(client) => client.get(uid)
            .await
            .map_err(|status| log::error!("couldn't fetch user info for {uid}: {status}"))
            .ok()
            .flatten()
            .and_then(|usr| usr.options)
            .and_then(|opts| opts.language_code),
        UserService::Disabled => None
    }
        .or(lang_code)
        .map(|code| match &code[..2] {
            "uk" | "be" => "ru".to_owned(),
            _ => code
        })
        .unwrap_or_else(|| {
            log::warn!("no language_code for {}, using the default", uid);
            "en".to_owned()
        })
}

pub fn get_full_name(user: &User) -> String {
    user.last_name.as_ref()
        .map(|last_name| format!("{} {}", user.first_name, last_name))
        .unwrap_or(user.first_name.clone())
}

#[cfg(test)]
mod tests {
    use teloxide::types::UserId;
    use crate::users::mock::UserServiceClientMock;
    use super::ensure_lang_code;

    #[tokio::test]
    async fn test_ensure_lang_code() {
        let uid = UserId(123456);
        let usr_client = UserServiceClientMock::new();
        assert_eq!(ensure_lang_code(uid, Some("ru".to_string()), &usr_client).await, "ru");
        assert_eq!(ensure_lang_code(uid, Some("be".to_string()), &usr_client).await, "ru");
        assert_eq!(ensure_lang_code(uid, None, &usr_client).await, "en")
    }
}