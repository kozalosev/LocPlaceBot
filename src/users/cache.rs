use std::marker::PhantomData;
use mobc::Manager;
use mobc_redis::redis::AsyncCommands;
use mobc_redis::redis::aio::ConnectionLike;

#[derive(Clone)]
pub struct RedisCache<K, V, RCM>
where
    K: redis::ToRedisArgs + Send + Sync,
    V: redis::FromRedisValue + redis::ToRedisArgs + Send + Sync,
    RCM: Manager,
    <RCM as Manager>::Connection: ConnectionLike,
    <RCM as Manager>::Error: std::fmt::Debug + std::fmt::Display
{
    pool: mobc::Pool<RCM>,
    key: String,

    _k: PhantomData<K>,
    _v: PhantomData<V>
}

impl <K, V, RCM> RedisCache<K, V, RCM>
where
    K: redis::ToRedisArgs + Send + Sync,
    V: redis::FromRedisValue + redis::ToRedisArgs + Send + Sync,
    RCM: Manager,
    <RCM as Manager>::Connection: ConnectionLike,
    <RCM as Manager>::Error: std::fmt::Debug + std::fmt::Display
{
    pub fn new(pool: mobc::Pool<RCM>, key: &str) -> Self {
        Self {
            pool,
            key: key.to_string(),
            _k: Default::default(),
            _v: Default::default(),
        }
    }

    pub async fn get(&self, key: K) -> anyhow::Result<V> {
        self.connection().await?
            .hget(self.key.clone(), key).await
            .map(Ok)?
    }

    pub async fn put(&self, key: K, value: V) -> anyhow::Result<()> {
        self.connection().await?
            .hset(self.key.clone(), key, value).await?;
        Ok(())
    }

    pub async fn delete(&self, key: K) -> anyhow::Result<()> {
        self.connection().await?
            .hdel(self.key.clone(), key).await?;
        Ok(())
    }

    async fn connection(&self) -> Result<RCM::Connection, mobc::Error<RCM::Error>> {
        Ok(self.pool.get().await?.into_inner())
    }
}
