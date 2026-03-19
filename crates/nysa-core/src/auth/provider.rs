use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;
use sea_orm::{ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, Set};

use crate::auth::{
    generate_token, hash_token, verify_token, compute_lookup_hash, LinkingCodeService, LinkingCodeError,
    RateLimiter, RateLimitResult, SessionManager, Session, SessionError,
};
use crate::database::entities::user::{ActiveModel as UserActiveModel, Column, Entity as UserEntity};

#[derive(Debug, Error)]
pub enum AuthError {
    #[error("Database error: {0}")]
    Database(#[from] sea_orm::DbErr),
    #[error("User not found: {0}")]
    UserNotFound(Uuid),
    #[error("Invalid token")]
    InvalidToken,
    #[error("Invalid linking code: {0}")]
    InvalidLinkingCode(#[from] LinkingCodeError),
    #[error("Linking code expired")]
    LinkingCodeExpired,
    #[error("Linking code already used")]
    LinkingCodeUsed,
    #[error("Platform already linked")]
    PlatformAlreadyLinked,
    #[error("Extension error: {0}")]
    Extension(String),
    #[error("Rate limited: {0}")]
    RateLimited(String),
    #[error("Session error: {0}")]
    SessionError(#[from] SessionError),
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
        let service = LinkingCodeService::new(_db.clone());
        service.generate_code(user_id, self.platform_name()).await
            .map_err(AuthError::InvalidLinkingCode)
    }
    
    async fn validate_platform_token(&self, _token: &str) -> Result<PlatformProfile, AuthError> {
        Err(AuthError::Extension("Not implemented".to_string()))
    }
}

pub struct AuthService {
    db: DatabaseConnection,
    linking_service: LinkingCodeService,
    session_manager: SessionManager,
    rate_limiter: RateLimiter,
}

impl AuthService {
    pub fn new(db: DatabaseConnection) -> Self {
        let rate_limiter = RateLimiter::new();
        rate_limiter.start_cleanup_task();
        
        Self {
            linking_service: LinkingCodeService::new(db.clone()),
            session_manager: SessionManager::new(db.clone()),
            rate_limiter,
            db,
        }
    }

    /// Check rate limits before auth operations
    pub fn check_rate_limit(&self, user_id: &str) -> RateLimitResult {
        self.rate_limiter.check_user(user_id)
    }

    /// Record an auth attempt
    pub fn record_auth_attempt(&self, user_id: &str) {
        self.rate_limiter.record_user_attempt(user_id);
    }

    /// Create a new user with a generated token
    pub async fn create_user(&self) -> Result<(Uuid, String), AuthError> {
        let user_id = Uuid::new_v4();
        let token = generate_token();
        
        let token_hash = hash_token(&token)
            .map_err(|e| AuthError::Database(sea_orm::DbErr::Custom(e.to_string())))?;
        let lookup_hash = compute_lookup_hash(&token);
        
        let user = UserActiveModel {
            id: Set(user_id),
            created_at: Set(chrono::Utc::now().naive_utc()),
            linked_profiles: Set(serde_json::json!({})),
            preferences: Set(serde_json::json!({})),
            token_hash: Set(token_hash),
            lookup_hash: Set(lookup_hash),
        };
        
        user.insert(&self.db).await?;
        
        tracing::info!("Created user {} with new token", user_id);
        
        Ok((user_id, token))
    }

    /// Authenticate using a token, returns user_id
    pub async fn authenticate(&self, token: &str) -> Result<Uuid, AuthError> {
        let token = token.trim();
        if !token.starts_with("nysa_") {
            return Err(AuthError::InvalidToken);
        }
        
        let lookup_hash = compute_lookup_hash(token);
        
        let user = UserEntity::find()
            .filter(Column::LookupHash.eq(lookup_hash))
            .one(&self.db)
            .await?
            .ok_or(AuthError::InvalidToken)?;
        
        if verify_token(token, &user.token_hash) {
            tracing::info!("Successfully authenticated user {}", user.id);
            Ok(user.id)
        } else {
            Err(AuthError::InvalidToken)
        }
    }

    /// Create a session after successful authentication
    pub async fn create_session(
        &self,
        user_id: Uuid,
        platform: &str,
        platform_session_id: &str,
        metadata: serde_json::Value,
    ) -> Result<Session, AuthError> {
        self.session_manager.create(user_id, platform, platform_session_id, metadata).await
            .map_err(AuthError::from)
    }

    /// Validate a session by ID
    pub async fn validate_session(&self, session_id: Uuid) -> Result<Session, AuthError> {
        self.session_manager.validate(session_id).await
            .map_err(AuthError::from)
    }

    /// Validate a session by platform session ID
    pub async fn validate_platform_session(
        &self,
        platform: &str,
        platform_session_id: &str,
    ) -> Result<Session, AuthError> {
        self.session_manager.validate_by_platform_session(platform, platform_session_id).await
            .map_err(AuthError::from)
    }

    /// Link a platform profile to a user
    pub async fn link_platform(
        &self, 
        user_id: Uuid, 
        platform: &str, 
        platform_id: &str,
        metadata: serde_json::Value,
    ) -> Result<(), AuthError> {
        let user = UserEntity::find_by_id(user_id)
            .one(&self.db)
            .await?
            .ok_or(AuthError::UserNotFound(user_id))?;

        // Check if platform already linked
        if let Some(profiles) = user.linked_profiles.as_object() {
            if profiles.contains_key(platform) {
                return Err(AuthError::PlatformAlreadyLinked);
            }
        }

        let mut profiles = user.linked_profiles.clone();
        
        if let Some(obj) = profiles.as_object_mut() {
            obj.insert(
                platform.to_string(),
                serde_json::json!({
                    "id": platform_id,
                    "metadata": metadata,
                    "linked_at": chrono::Utc::now().to_rfc3339(),
                }),
            );
        }

        let mut active_user: UserActiveModel = user.into();
        active_user.linked_profiles = Set(profiles);
        active_user.update(&self.db).await?;

        tracing::info!("Linked platform {} (id: {}) to user {}", platform, platform_id, user_id);
        
        Ok(())
    }

    /// Get user's linked profiles
    pub async fn get_user_profiles(&self, user_id: &Uuid) -> Result<serde_json::Value, AuthError> {
        let user = UserEntity::find_by_id(*user_id)
            .one(&self.db)
            .await?
            .ok_or(AuthError::UserNotFound(*user_id))?;

        Ok(user.linked_profiles)
    }

    /// Redeem a linking code
    pub async fn redeem_linking_code(
        &self, 
        code: &str, 
        platform: &str, 
        platform_id: &str,
        metadata: serde_json::Value,
    ) -> Result<Uuid, AuthError> {
        self.linking_service.redeem_code(code, platform, platform_id, metadata).await
            .map_err(AuthError::InvalidLinkingCode)
    }

    /// Generate a linking code for a user
    pub async fn generate_linking_code(
        &self,
        user_id: Uuid,
        platform: &str,
    ) -> Result<String, AuthError> {
        self.linking_service.generate_code(user_id, platform).await
            .map_err(AuthError::InvalidLinkingCode)
    }

    /// Find user by platform ID
    /// Note: This is O(n) with current schema. For better performance,
    /// a separate platform_links table with proper indexing would be needed.
    pub async fn find_user_by_platform(
        &self, 
        platform: &str, 
        platform_id: &str,
    ) -> Result<Option<Uuid>, AuthError> {
        let users = UserEntity::find().all(&self.db).await?;

        for user in users {
            if let Some(profiles) = user.linked_profiles.as_object() {
                if let Some(platform_data) = profiles.get(platform) {
                    if let Some(id) = platform_data.get("id") {
                        if let Some(id_str) = id.as_str() {
                            if id_str == platform_id {
                                return Ok(Some(user.id));
                            }
                        }
                    }
                }
            }
        }

        Ok(None)
    }

    /// Get user by ID
    pub async fn get_user(&self, user_id: Uuid) -> Result<crate::database::entities::user::Model, AuthError> {
        UserEntity::find_by_id(user_id)
            .one(&self.db)
            .await?
            .ok_or(AuthError::UserNotFound(user_id))
    }

    /// Revoke user session
    pub async fn revoke_session(&self, session_id: Uuid) -> Result<(), AuthError> {
        self.session_manager.revoke(session_id).await
            .map_err(AuthError::from)
    }

    /// Get all active sessions for a user
    pub async fn get_user_sessions(&self, user_id: Uuid) -> Result<Vec<Session>, AuthError> {
        self.session_manager.get_user_sessions(user_id).await
            .map_err(AuthError::from)
    }

    /// Clean up expired data (sessions, linking codes)
    pub async fn cleanup_expired(&self) -> Result<(u64, u64), AuthError> {
        let sessions_cleaned = self.session_manager.cleanup_expired().await?;
        let codes_cleaned = self.linking_service.cleanup_expired().await?;
        
        Ok((sessions_cleaned, codes_cleaned))
    }
}
