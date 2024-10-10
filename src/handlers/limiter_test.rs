use mobc::Pool;
use mobc_redis::redis::Client;
use mobc_redis::RedisConnectionManager;
use teloxide::prelude::UserId;
use testcontainers::{clients, Container, core::WaitFor, images::generic::GenericImage};
use crate::handlers::limiter::{GetUserId, RequestsLimiter};

#[tokio::test]
async fn test_rate_limiter() {
    pretty_env_logger::init();

    let docker = clients::Cli::default();
    let (_redis_container, redis_client) = start_redis(&docker);
    let limiter = RequestsLimiter::new(redis_client, 2, 60);
    let entity = &Entity{};

    // the first two are allowed
    assert!(limiter.is_req_allowed(entity).await);
    assert!(limiter.is_req_allowed(entity).await);
    // the third is forbidden
    assert!(!limiter.is_req_allowed(entity).await);
}

fn start_redis(docker: &clients::Cli) -> (Container<GenericImage>, Pool<RedisConnectionManager>) {
    let redis_image = GenericImage::new("redis", "latest")
        .with_exposed_port(6379)
        .with_wait_for(WaitFor::message_on_stdout("Ready to accept connections"));

    let redis_container = docker.run(redis_image);
    let redis_port = redis_container.get_host_port_ipv4(6379);
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
