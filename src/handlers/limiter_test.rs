use teloxide::prelude::UserId;
use crate::handlers::limiter::{GetUserId, RequestsLimiter};
use crate::testutils::start_redis;

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

struct Entity {}

impl GetUserId for Entity {
    fn user_id(&self) -> Option<UserId> {
        Some(UserId(123456))
    }
}
