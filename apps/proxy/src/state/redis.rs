use std::collections::HashMap;

use anyhow::Result;
use redis::{AsyncCommands, Client};

#[derive(Clone)]
pub struct RedisState {
    client: Client,
    key_prefix: String,
}

impl RedisState {
    pub async fn new(redis_url: &str) -> Result<Self> {
        let client = Client::open(redis_url)?;
        Ok(Self {
            client,
            key_prefix: "maskproxy:session".to_string(),
        })
    }

    pub async fn save_mapping(
        &self,
        session_id: &str,
        mapping: &HashMap<String, String>,
        ttl_secs: u64,
    ) -> Result<()> {
        let mut connection = self.client.get_multiplexed_tokio_connection().await?;
        let payload = serde_json::to_string(mapping)?;
        let _: () = connection
            .set_ex(self.key(session_id), payload, ttl_secs)
            .await?;
        Ok(())
    }

    pub async fn get_value(&self, key: &str) -> Result<Option<String>> {
        let mut connection = self.client.get_multiplexed_tokio_connection().await?;
        let value: Option<String> = connection.get(key).await?;
        Ok(value)
    }

    pub async fn set_value(&self, key: &str, value: &str, ttl_secs: u64) -> Result<()> {
        let mut connection = self.client.get_multiplexed_tokio_connection().await?;
        let _: () = connection.set_ex(key, value, ttl_secs).await?;
        Ok(())
    }

    pub async fn get_mapping(&self, session_id: &str) -> Result<HashMap<String, String>> {
        let mut connection = self.client.get_multiplexed_tokio_connection().await?;
        let payload: Option<String> = connection.get(self.key(session_id)).await?;

        match payload {
            Some(payload) => Ok(serde_json::from_str(&payload)?),
            None => Ok(HashMap::new()),
        }
    }

    pub async fn delete_session(&self, session_id: &str) -> Result<()> {
        let mut connection = self.client.get_multiplexed_tokio_connection().await?;
        let _: usize = connection.del(self.key(session_id)).await?;
        Ok(())
    }

    fn key(&self, session_id: &str) -> String {
        session_key(&self.key_prefix, session_id)
    }
}

fn session_key(key_prefix: &str, session_id: &str) -> String {
    format!("{}:{}", key_prefix, session_id)
}

#[cfg(test)]
mod tests {
    use super::session_key;

    #[test]
    fn redis_key_uses_session_prefix() {
        assert_eq!(session_key("maskproxy:session", "abc123"), "maskproxy:session:abc123");
    }
}
