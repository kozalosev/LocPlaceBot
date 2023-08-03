use teloxide::prelude::UserId;

pub fn ensure_lang_code(uid: UserId, lang_code: Option<String>) -> String {
    lang_code
        .unwrap_or_else(|| {
            log::warn!("no language_code for {}, using the default", uid);
            String::default()
        })
}

#[cfg(test)]
mod tests {
    use teloxide::types::UserId;
    use super::ensure_lang_code;

    #[test]
    fn test_ensure_lang_code() {
        let uid = UserId(123456);
        assert_eq!(ensure_lang_code(uid, Some("ru".to_string())), "ru");
        assert_eq!(ensure_lang_code(uid, None), "")
    }
}