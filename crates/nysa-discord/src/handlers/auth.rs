use nysa_core::auth::Session;
use nysa_core::{AuthError, AuthService};
use sea_orm::DatabaseConnection;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::models::user::Entity as UserEntity;
use sea_orm::EntityTrait;

pub struct AuthenticatedUser {
    pub user_id: Uuid,
    pub discord_id: u64,
    pub username: String,
    pub session: Option<Session>,
}

pub struct AuthMiddleware {
    db: DatabaseConnection,
    auth_service: AuthService,
    cache: Arc<RwLock<std::collections::HashMap<u64, (AuthenticatedUser, std::time::Instant)>>>,
    cache_ttl: std::time::Duration,
}

impl AuthMiddleware {
    pub fn new(db: DatabaseConnection) -> Self {
        let cache = Arc::new(RwLock::new(std::collections::HashMap::new()));
        let cache_ttl = std::time::Duration::from_secs(300); // 5 minutes

        // Start cache cleanup task
        let cache_clone = Arc::clone(&cache);
        let cache_ttl_clone = cache_ttl;
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(60));
            loop {
                interval.tick().await;
                let now = std::time::Instant::now();
                let mut cache = cache_clone.write().await;
                cache.retain(|_, (_, timestamp)| now.duration_since(*timestamp) < cache_ttl_clone);
            }
        });

        Self {
            db: db.clone(),
            auth_service: AuthService::new(db),
            cache,
            cache_ttl,
        }
    }

    /// Check if a Discord user is authenticated
    /// Returns the authenticated user info if found
    pub async fn authenticate(
        &self,
        discord_id: u64,
        username: String,
    ) -> Option<AuthenticatedUser> {
        // Check cache first
        {
            let cache = self.cache.read().await;
            if let Some((user, timestamp)) = cache.get(&discord_id)
                && std::time::Instant::now().duration_since(*timestamp) < self.cache_ttl
            {
                return Some(user.clone());
            }
        }

        // Find user in database
        let users = match UserEntity::find().all(&self.db).await {
            Ok(u) => u,
            Err(e) => {
                tracing::error!("Database error in auth middleware: {}", e);
                return None;
            }
        };

        let user = users.into_iter().find(|u| {
            if let Some(profiles) = u.linked_profiles.as_object()
                && let Some(discord) = profiles.get("discord")
                && let Some(id) = discord.get("id")
                && let Some(id_str) = id.as_str()
            {
                return id_str == discord_id.to_string();
            }
            false
        });

        if let Some(user) = user {
            // Try to find or create session
            let session = match self
                .auth_service
                .validate_platform_session("discord", &discord_id.to_string())
                .await
            {
                Ok(s) => Some(s),
                Err(_) => {
                    // Create new session
                    let metadata = serde_json::json!({
                        "username": username,
                    });
                    match self
                        .auth_service
                        .create_session(user.id, "discord", &discord_id.to_string(), metadata)
                        .await
                    {
                        Ok(s) => Some(s),
                        Err(e) => {
                            tracing::error!("Failed to create session: {}", e);
                            None
                        }
                    }
                }
            };

            let auth_user = AuthenticatedUser {
                user_id: user.id,
                discord_id,
                username: username.clone(),
                session,
            };

            // Cache the result
            let mut cache = self.cache.write().await;
            cache.insert(discord_id, (auth_user.clone(), std::time::Instant::now()));

            Some(auth_user)
        } else {
            None
        }
    }

    /// Require authentication - returns error if not authenticated
    pub async fn require_auth(
        &self,
        discord_id: u64,
        username: String,
    ) -> Result<AuthenticatedUser, AuthError> {
        self.authenticate(discord_id, username)
            .await
            .ok_or(AuthError::InvalidToken)
    }

    /// Get user by UUID
    pub async fn get_user_by_id(&self, user_id: Uuid) -> Option<crate::models::user::Model> {
        match UserEntity::find_by_id(user_id).one(&self.db).await {
            Ok(u) => u,
            Err(e) => {
                tracing::error!("Database error: {}", e);
                None
            }
        }
    }

    /// Clear cache for a specific user
    pub async fn clear_cache(&self, discord_id: u64) {
        let mut cache = self.cache.write().await;
        cache.remove(&discord_id);
    }
}

impl Clone for AuthMiddleware {
    fn clone(&self) -> Self {
        Self {
            db: self.db.clone(),
            auth_service: AuthService::new(self.db.clone()),
            cache: Arc::clone(&self.cache),
            cache_ttl: self.cache_ttl,
        }
    }
}

impl Clone for AuthenticatedUser {
    fn clone(&self) -> Self {
        Self {
            user_id: self.user_id,
            discord_id: self.discord_id,
            username: self.username.clone(),
            session: self.session.clone(),
        }
    }
}
