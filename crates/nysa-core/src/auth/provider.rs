use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;
use sea_orm::DatabaseConnection;

#[derive(Debug, Error)]
pub enum AuthError {
    #[error("Database error: {0}")]
    Database(String),
    #[error("User not found: {0}")]
    UserNotFound(Uuid),
    #[error("Invalid token")]
    InvalidToken,
    #[error("Invalid linking code")]
    InvalidLinkingCode,
    #[error("Linking code expired")]
    LinkingCodeExpired,
    #[error("Linking code already used")]
    LinkingCodeUsed,
    #[error("Platform already linked")]
    PlatformAlreadyLinked,
    #[error("Extension error: {0}")]
    Extension(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlatformProfile {
    pub platform: String,
    pub platform_id: String,
    pub display_name: Option<String>,
    pub metadata: serde_json::Value,
}

#[async_trait]
pub trait AuthProvider: Send + Sync {
    fn platform_name(&self) -> &'static str;
    
    async fn get_platform_id(&self, context: &str) -> Option<String>;
    
    async fn generate_linking_code(&self, user_id: Uuid, _db: &DatabaseConnection) -> Result<String, AuthError> {
        let token = crate::auth::generate_token();
        Ok(token)
    }
    
    async fn validate_platform_token(&self, _token: &str) -> Result<PlatformProfile, AuthError> {
        Err(AuthError::Extension("Not implemented".to_string()))
    }
}

pub struct AuthService {
    db: DatabaseConnection,
}

impl AuthService {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }

    pub async fn create_user(&self, _token: &str) -> Result<(Uuid, String), AuthError> {
        let user_id = Uuid::new_v4();
        let token = crate::auth::generate_token();
        
        let token_hash = crate::auth::hash_token(&token)
            .map_err(|e| AuthError::Database(e.to_string()))?;
        
        tracing::info!("Created user {} with token_hash", user_id);
        
        Ok((user_id, token))
    }

    pub async fn authenticate(&self, token: &str) -> Result<Uuid, AuthError> {
        use crate::auth::verify_token;
        
        let token = token.trim();
        if !token.starts_with("nysa_") {
            return Err(AuthError::InvalidToken);
        }
        
        tracing::info!("Authentication attempt for token starting with nysa_");
        
        Err(AuthError::InvalidToken)
    }

    pub async fn link_platform(
        &self, 
        _user_id: Uuid, 
        _platform: &str, 
        _platform_id: &str,
        _metadata: serde_json::Value,
    ) -> Result<(), AuthError> {
        Ok(())
    }

    pub async fn get_user_profiles(&self, _user_id: &Uuid) -> Result<serde_json::Value, AuthError> {
        Ok(serde_json::json!({}))
    }

    pub async fn use_linking_code(
        &self, 
        _code: &str, 
        _platform: &str, 
        _platform_id: &str,
        _metadata: serde_json::Value,
    ) -> Result<Uuid, AuthError> {
        Err(AuthError::InvalidLinkingCode)
    }

    pub async fn find_user_by_platform(&self, _platform: &str, _platform_id: &str) -> Result<Option<Uuid>, AuthError> {
        Ok(None)
    }
}
