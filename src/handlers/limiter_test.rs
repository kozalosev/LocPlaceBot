use mobc::Pool;
use mobc_redis::redis::Client;
use mobc_redis::RedisConnectionManager;
use teloxide::prelude::UserId;
use testcontainers::{core::WaitFor, GenericImage, ContainerAsync};
use testcontainers::core::IntoContainerPort;
use testcontainers::runners::AsyncRunner;
use crate::handlers::limiter::{GetUserId, RequestsLimiter};

#[tokio::test]
async fn test_rate_limiter() {
    pretty_env_logger::init();
    
    let (_redis_container, redis_client) = start_redis().await;
    let limiter = RequestsLimiter::new(redis_client, 2, 60);
    let entity = &Entity{};

    // the first two are allowed
    assert!(limiter.is_req_allowed(entity).await);
    assert!(limiter.is_req_allowed(entity).await);
    // the third is forbidden
    assert!(!limiter.is_req_allowed(entity).await);
}

async fn start_redis() -> (ContainerAsync<GenericImage>, Pool<RedisConnectionManager>) {
    let redis_container = GenericImage::new("redis", "latest")
        .with_exposed_port(6379.tcp())
        .with_wait_for(WaitFor::message_on_stdout("Ready to accept connections"))
        .start()
        .await
        .expect("Couldn't start Redis");
    let redis_port = redis_container.get_host_port_ipv4(6379).await
        .expect("Couldn't fetch redis port");
    let redis_client = Client::open(format!("redis://127.0.0.1:{redis_port}"))
        .map(RedisConnectionManager::new)
        .map(Pool::new)
        .expect("couldn't establish a connection with Redis");
    (redis_container, redis_client)
}

struct Entity {}

impl GetUserId for Entity {
    fn user_id(&self) -> Option<UserId> {
        Some(UserId(123456))
    }
}
