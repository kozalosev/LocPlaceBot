#[cfg(test)]
pub mod mock;

use std::net::{SocketAddr, ToSocketAddrs};
use std::sync::Arc;
use anyhow::anyhow;
use async_trait::async_trait;
use chashmap::CHashMap;
use derive_more::{Constructor, Display, From};
use once_cell::sync::Lazy;
use serde_json::json;
use teloxide::types::{MessageId, UserId};
use tonic::{Code, Response};
use tonic::transport::Channel;
use generated::user_service_client::UserServiceClient as GrpcClient;
use generated::update_user_request::Target;
use generated::*;

pub mod generated {
    tonic::include_proto!("user_service");
}

const ENV_GRPC_ADDR_USER_SERVICE: &str = "GRPC_ADDR_USER_SERVICE";

static USER_CACHE_TIME_SECS: Lazy<u64> = Lazy::new(|| std::env::var("USER_CACHE_TIME_SECS")
    .ok()
    .and_then(|v| v.parse()
        .map_err(|e| log::error!("invalid value for USER_CACHE_TIME_SECS: {e}"))
        .ok())
    .unwrap_or(360));

pub struct Hello {
    service_name: String
}

impl From<&str> for Hello {
    fn from(value: &str) -> Self {
        Self { service_name: value.to_owned() }
    }
}

impl Into<Service> for Hello {
    fn into(self) -> Service {
        Service {
            name: self.service_name,
            kind: ServiceType::TelegramBot as i32,
        }
    }
}

#[derive(Constructor)]
pub struct Consent {
    message_id: MessageId,
    eula_hash: &'static str,
}

impl Into<serde_json::Value> for Consent {
    fn into(self) -> serde_json::Value {
        json!({
            "user_agreement": {
                "hash": self.eula_hash,
                "message_id": self.message_id
            },
            "accepted_by": "click-on-inline-button"
        })
    }
}

#[derive(Clone)]
struct CachedUser {
    user: Option<User>,
    updated_at: tokio::time::Instant,
}

impl From<Option<User>> for CachedUser {
    fn from(value: Option<User>) -> Self {
        Self {
            user: value,
            updated_at: tokio::time::Instant::now(),
        }
    }
}

impl From<User> for CachedUser {
    fn from(value: User) -> Self {
        Some(value).into()
    }
}

#[derive(Debug, Display, From, thiserror::Error)]
pub enum Unsupported {
    RegistrationStatus(i32),
}

#[derive(Debug, Display, From, derive_more::Error)]
pub enum RequestError {
    Status(tonic::Status),
    Unsupported(Unsupported),
    Internal(Box<dyn std::error::Error + Send + Sync + 'static>),
}

impl RequestError {
    fn internal(e: impl std::error::Error + Send + Sync + 'static) -> Self {
        Self::Internal(Box::new(e))
    }
}

#[async_trait]
pub trait UserServiceClient : Clone {
    async fn get(&self, uid: UserId) -> Result<Option<User>, tonic::Status>;
    async fn register(&self, uid: UserId, name: String, consent: Consent) -> Result<i64, RequestError>;
    async fn set_language(&self, uid: UserId, code: &str) -> Result<(), tonic::Status>;
    async fn set_location(&self, uid: UserId, latitude: f64, longitude: f64) -> Result<(), tonic::Status>;
}

#[derive(Clone, From)]
pub enum UserService<T: UserServiceClient> {
    Connected(T),
    Disabled
}

impl <T: UserServiceClient> UserService<T> {
    pub fn enabled(&self) -> bool {
        match self {
            Self::Connected(_) => true,
            Self::Disabled => false
        }
    }

    pub fn disabled(&self) -> bool {
        !self.enabled()
    }

    pub fn unwrap(self) -> T {
        match self {
            Self::Connected(inner) => inner,
            Self::Disabled => panic!("attempt to unwrap a disabled UserService")
        }
    }
}

#[derive(Clone)]
pub struct UserServiceClientGrpc {
    inner: GrpcClient<Channel>,
    cache: Arc<CHashMap<UserId, CachedUser>>,
    service_descr: Service,
}

impl UserServiceClientGrpc {
    pub async fn connect(addr: impl Into<SocketAddr>, hello: Hello) -> Result<Self, tonic::transport::Error> {
        Ok(Self {
            inner: GrpcClient::connect(format!("http://{}", addr.into())).await?,
            cache: Arc::new(Default::default()),
            service_descr: hello.into(),
        })
    }

    pub async fn with_addr_from_env(hello: Hello) -> anyhow::Result<Self> {
        let addr = std::env::var(ENV_GRPC_ADDR_USER_SERVICE)?
            .to_socket_addrs()?.next()
            .ok_or(anyhow!("GRPC_ADDR_USER_SERVICE is not specified!"))?;
        let client = Self::connect(addr, hello).await?;
        Ok(client)
    }

    pub fn clean_up_cache(&self) {
        self.cache.retain(|_, usr| is_user_fresh(usr));
    }

    async fn get_internal_id(&self, uid: UserId) -> Result<i64, tonic::Status> {
        self.get(uid).await?
            .map(|u| u.id)
            .ok_or(tonic::Status::not_found("user not found"))
    }
}

#[async_trait]
impl UserServiceClient for UserServiceClientGrpc {
    async fn get(&self, uid: UserId) -> Result<Option<User>, tonic::Status> {
        let cached_user = self.cache
            .get(&uid)
            .filter(|usr| is_user_fresh(usr))
            .map(|usr| usr.clone());
        let maybe_usr = match cached_user {
            Some(cached) => cached.user,
            None => {
                let resp = self.inner.clone().get(GetUserRequest {
                    id: uid.0 as i64,
                    by_external_id: true,
                }).await;
                match resp {
                    Ok(resp_user) => {
                        let usr = resp_user.into_inner();
                        self.cache.insert(uid, usr.clone().into());
                        Some(usr)
                    },
                    Err(status) if status.code() == Code::NotFound => {
                        self.cache.insert(uid, None.into());
                        None
                    },
                    Err(status) => Err(status)?
                }
            }
        };
        Ok(maybe_usr)
    }

    async fn register(&self, uid: UserId, name: String, consent: Consent) -> Result<i64, RequestError> {
        let user = ExternalUser {
            external_id: uid.0 as i64,
            name: Some(name),
        };
        let consent_info = serde_json::from_value(consent.into())
            .map_err(RequestError::internal)?;
        let response = self.inner.clone().register(RegistrationRequest {
            user: Some(user),
            service: Some(self.service_descr.clone()),
            consent_info: Some(consent_info),
        }).await.map(Response::into_inner)?;

        let status = RegistrationStatus::try_from(response.status)
            .map_err(|_| Unsupported::RegistrationStatus(response.status))?;
        match status {
            RegistrationStatus::Unspecified => Err(Unsupported::RegistrationStatus(0))?,
            RegistrationStatus::Created => {
                log::info!("a user with ID {uid} was registered successfully!");
                self.cache.remove(&uid);
                Ok(response.id)
            },
            RegistrationStatus::AlreadyPresent => {
                log::warn!("attempt to register a user with ID {uid} once again");
                self.cache.remove(&uid);
                Ok(response.id)
            }
        }
    }

    async fn set_language(&self, uid: UserId, code: &str) -> Result<(), tonic::Status> {
        let id = self.get_internal_id(uid).await?;
        self.inner.clone().update(UpdateUserRequest {
            id,
            target: Some(Target::Language(code.to_owned())),
        }).await?;
        self.cache.remove(&uid);
        Ok(())
    }

    async fn set_location(&self, uid: UserId, latitude: f64, longitude: f64) -> Result<(), tonic::Status> {
        let id = self.get_internal_id(uid).await?;
        let location = Location { latitude, longitude };
        self.inner.clone().update(UpdateUserRequest {
            id,
            target: Some(Target::Location(location)),
        }).await?;
        self.cache.remove(&uid);
        Ok(())
    }
}

fn is_user_fresh(usr: &CachedUser) -> bool {
    let time_difference = tokio::time::Instant::now() - usr.updated_at;
    time_difference.as_secs() <= *USER_CACHE_TIME_SECS
}
