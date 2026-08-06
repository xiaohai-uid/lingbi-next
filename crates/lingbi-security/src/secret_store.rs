use async_trait::async_trait;
use lingbi_contracts::{AppError, ErrorCode};
use std::collections::HashMap;
use std::fmt;
use std::sync::Mutex;

#[derive(Clone)]
pub struct SecretString(String);

impl SecretString {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for SecretString {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("<redacted>")
    }
}

#[async_trait]
pub trait SecretStore: Send + Sync {
    async fn put(&self, key: &str, secret: SecretString) -> Result<(), AppError>;
    async fn get(&self, key: &str) -> Result<Option<SecretString>, AppError>;
    async fn delete(&self, key: &str) -> Result<(), AppError>;
}

#[derive(Default)]
pub struct MemorySecretStore {
    inner: Mutex<HashMap<String, SecretString>>,
}

#[async_trait]
impl SecretStore for MemorySecretStore {
    async fn put(&self, key: &str, secret: SecretString) -> Result<(), AppError> {
        self.inner
            .lock()
            .map_err(|_| lock_error())?
            .insert(key.to_owned(), secret);
        Ok(())
    }

    async fn get(&self, key: &str) -> Result<Option<SecretString>, AppError> {
        Ok(self
            .inner
            .lock()
            .map_err(|_| lock_error())?
            .get(key)
            .cloned())
    }

    async fn delete(&self, key: &str) -> Result<(), AppError> {
        self.inner.lock().map_err(|_| lock_error())?.remove(key);
        Ok(())
    }
}

fn lock_error() -> AppError {
    AppError::new(
        ErrorCode::ProjectCorrupted,
        "secret store lock poisoned".to_owned(),
        false,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn secret_store_put_get_delete_round_trip() {
        let store = MemorySecretStore::default();
        store
            .put("openai", SecretString::new("sk-secret"))
            .await
            .expect("put");

        let secret = store.get("openai").await.expect("get").expect("value");
        assert_eq!(secret.expose(), "sk-secret");

        store.delete("openai").await.expect("delete");
        assert!(store.get("openai").await.expect("get").is_none());
    }

    #[test]
    fn secret_string_debug_is_redacted() {
        let secret = SecretString::new("sk-secret");
        assert!(!format!("{secret:?}").contains("sk-secret"));
    }
}
