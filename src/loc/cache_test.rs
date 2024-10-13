use std::sync::Arc;
use http::Method;
use reqwest::Body;
use reqwest_middleware::ClientWithMiddleware;
use crate::loc::cache::caching_client_builder;
use crate::testutils::start_redis;

#[tokio::test]
async fn test_cache() {
    let (_redis_container, redis_pool) = start_redis().await;
    let middleware = Arc::new(mock::RequestStoppingCounter::default());
    let client = caching_client_builder(&redis_pool)
        .with_arc(middleware.clone())
        .build();

    send_get_request(&client).await;
    send_get_request(&client).await;
    middleware.verify_requests_count(1);

    middleware.reset();

    send_request(Method::POST, &client, "req-1").await;
    send_request(Method::POST, &client, "req-1").await;
    send_request(Method::POST, &client, "req-2").await;
    middleware.verify_requests_count(2);
}

async fn send_get_request(client: &ClientWithMiddleware) {
    send_request(Method::GET, client, "").await
}

async fn send_request(method: Method, client: &ClientWithMiddleware, body: &str) {
    client.request(method, "https://foo.bar")
        .body(Body::from(body.to_string()))
        .send().await
        .expect("couldn't send request");
}

pub mod mock {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use axum::http::Extensions;
    use http::header::CACHE_CONTROL;
    use http::HeaderValue;
    use reqwest::{Body, Request};
    use reqwest_middleware::{Middleware, Next};

    #[derive(Default)]
    pub(super) struct RequestStoppingCounter {
        counter: AtomicUsize,
    }
    
    impl RequestStoppingCounter {
        pub fn verify_requests_count(&self, count: usize) {
            assert_eq!(count, self.counter.load(Ordering::Acquire));
        }
        
        pub fn reset(&self) {
            self.counter.store(0, Ordering::Release);
        }
    }

    #[async_trait::async_trait]
    impl Middleware for RequestStoppingCounter {
        async fn handle(&self, _req: Request, _extensions: &mut Extensions, _next: Next<'_>) -> reqwest_middleware::Result<reqwest::Response> {
            self.counter.fetch_add(1, Ordering::Relaxed);

            let mut resp = http::Response::new(Body::from("200 OK"));
            resp.headers_mut().insert(CACHE_CONTROL, HeaderValue::from_static("max-age=604800"));
            Ok(reqwest::Response::from(resp))
        }
    }
}
