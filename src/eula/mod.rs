use once_cell::sync::Lazy;

static  EN_EULA_TEXT: &str = include_str!("en.html");
static  RU_EULA_TEXT: &str = include_str!("ru.html");

static EN_EULA_HASH: Lazy<String> = Lazy::new(|| sha256::digest(EN_EULA_TEXT));
static RU_EULA_HASH: Lazy<String> = Lazy::new(|| sha256::digest(RU_EULA_TEXT));

pub struct EndUserAgreement {
    pub text: &'static str,
    pub hash: &'static str,
}

pub static EULA_EN: Lazy<EndUserAgreement> = Lazy::new(|| {
    EndUserAgreement {
        text: EN_EULA_TEXT,
        hash: &EN_EULA_HASH,
    }
});

pub static EULA_RU: Lazy<EndUserAgreement> = Lazy::new(|| EndUserAgreement {
    text: RU_EULA_TEXT,
    hash: &RU_EULA_HASH,
});

pub fn get_in(lang_code: &str) -> &EndUserAgreement {
    match lang_code {
        "ru" => &EULA_RU,
        _    => &EULA_EN
    }
}
