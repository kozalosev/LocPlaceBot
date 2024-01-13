use std::sync::Arc;
use async_trait::async_trait;
use chashmap::CHashMap;
use teloxide::prelude::UserId;
use tonic::Status;
use crate::users::generated::{Location, User};
use crate::users::generated::user::Options;
use super::{Consent, RequestError, UserService, UserServiceClient};

#[derive(Clone)]
pub struct UserServiceClientMock {
    users: Arc<CHashMap<UserId, User>>
}

impl UserServiceClientMock {
    pub fn new() -> UserService<Self> {
        UserService::Connected(Self {
            users: Arc::new(CHashMap::new())
        })
    }

    fn change_option(&self, uid: UserId, opts_builder: impl FnOnce(Options) -> Options) -> Result<(), Status> {
        let mut user= self.users.get(&uid)
            .map(|u| u.clone())
            .ok_or(Status::not_found("user"))?;
        let opts = user.options.unwrap_or_default();

        user.options = Some(opts_builder(opts));
        Ok(())
    }
}

#[async_trait]
impl UserServiceClient for UserServiceClientMock {
    async fn get(&self, uid: UserId) -> Result<Option<User>, Status> {
        Ok(self.users.get(&uid).map(|u| u.clone()))
    }

    async fn register(&self, uid: UserId, name: String, _: Consent) -> Result<i64, RequestError> {
        self.users.insert(uid, User {
            id: 1,
            name: Some(name),
            options: Some(Options::default()),
            is_premium: false,
        });
        Ok(1)
    }

    async fn set_language(&self, uid: UserId, code: &str) -> Result<(), Status> {
        self.change_option(uid, |opts| Options {
            language_code: Some(code.to_owned()),
            ..opts
        })
    }

    async fn set_location(&self, uid: UserId, latitude: f64, longitude: f64) -> Result<(), Status> {
        self.change_option(uid, |opts| Options {
            location: Some(Location { latitude, longitude }),
            ..opts
        })
    }
}
