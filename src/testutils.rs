use mobc::Pool;
use mobc_redis::redis::Client;
use mobc_redis::RedisConnectionManager;
use testcontainers::{ContainerAsync, GenericImage};
use testcontainers::core::{IntoContainerPort, WaitFor};
use testcontainers::runners::AsyncRunner;

pub async fn start_redis() -> (ContainerAsync<GenericImage>, Pool<RedisConnectionManager>) {
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
