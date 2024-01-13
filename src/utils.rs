use teloxide::prelude::UserId;
use teloxide::types::User;
use crate::users::{UserService, UserServiceClient};
use crate::users::generated::user::Options;

pub async fn ensure_lang_code(uid: UserId, lang_code: Option<String>, usr_srv_client: &UserService<impl UserServiceClient>) -> String {
    try_fetch_user_info(uid, usr_srv_client).await
        .and_then(|opts| opts.language_code)
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

pub async fn try_determine_location(uid: UserId, usr_srv_client: &UserService<impl UserServiceClient>) -> Option<(f64, f64)> {
    try_fetch_user_info(uid, usr_srv_client).await
        .and_then(|opts| opts.location)
        .map(|loc| (loc.latitude, loc.longitude))
}

pub fn get_full_name(user: &User) -> String {
    user.last_name.as_ref()
        .map(|last_name| format!("{} {}", user.first_name, last_name))
        .unwrap_or(user.first_name.clone())
}

async fn try_fetch_user_info(uid: UserId, usr_srv_client: &UserService<impl UserServiceClient>) -> Option<Options> {
    match usr_srv_client {
        UserService::Connected(client) => client.get(uid)
            .await
            .map_err(|status| log::error!("couldn't fetch user info for {uid}: {status}"))
            .ok()
            .flatten()
            .and_then(|usr| usr.options),
        UserService::Disabled => None
    }
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