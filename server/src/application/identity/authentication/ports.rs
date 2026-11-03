use async_trait::async_trait;

#[async_trait]
pub trait SessionRepository: Send + Sync {
    async fn create(&self, user_id: &str, refresh_hash: &str, access_hash: &str) -> anyhow::Result<()>;
    /// Atomically attach an access token only if the session still exists.
    async fn add_access(&self, user_id: &str, refresh_hash: &str, access_hash: &str) -> anyhow::Result<bool>;
    async fn contains_access(&self, user_id: &str, access_hash: &str) -> anyhow::Result<bool>;
    /// Idempotently revoke the session and every access token belonging to it.
    async fn revoke(&self, user_id: &str, refresh_hash: &str) -> anyhow::Result<()>;
}
