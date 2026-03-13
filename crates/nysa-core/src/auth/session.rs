use chrono::{DateTime, Utc};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, Set,
};
use std::time::Duration;
use thiserror::Error;
use uuid::Uuid;

use crate::database::entities::session::{ActiveModel as SessionActiveModel, Column};
use crate::database::entities::session::Entity as SessionEntity;

const DEFAULT_SESSION_DURATION_DAYS: i64 = 30;

#[derive(Debug, Error)]
pub enum SessionError {
    #[error("Database error: {0}")]
    Database(#[from] sea_orm::DbErr),
    #[error("Session not found")]
    NotFound,
    #[error("Session expired")]
    Expired,
    #[error("Session revoked")]
    Revoked,
    #[error("Invalid session data")]
    InvalidData,
}

#[derive(Debug, Clone)]
pub struct Session {
    pub id: Uuid,
    pub user_id: Uuid,
    pub platform: String,
    pub platform_session_id: String,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub metadata: serde_json::Value,
}

impl From<crate::database::entities::session::Model> for Session {
    fn from(model: crate::database::entities::session::Model) -> Self {
        Session {
            id: model.id,
            user_id: model.user_id,
            platform: model.platform,
            platform_session_id: model.platform_session_id,
            created_at: DateTime::from_naive_utc_and_offset(model.created_at, Utc),
            expires_at: DateTime::from_naive_utc_and_offset(model.expires_at, Utc),
            metadata: model.metadata,
        }
    }
}

pub struct SessionManager {
    db: DatabaseConnection,
    default_duration: Duration,
}

impl SessionManager {
    pub fn new(db: DatabaseConnection) -> Self {
        Self {
            db,
            default_duration: Duration::from_secs(DEFAULT_SESSION_DURATION_DAYS as u64 * 24 * 60 * 60),
        }
    }

    pub fn with_duration(db: DatabaseConnection, duration: Duration) -> Self {
        Self {
            db,
            default_duration: duration,
        }
    }

    /// Create a new session for a user
    pub async fn create(
        &self,
        user_id: Uuid,
        platform: &str,
        platform_session_id: &str,
        metadata: serde_json::Value,
    ) -> Result<Session, SessionError> {
        let now = Utc::now();
        let expires_at = now + chrono::Duration::from_std(self.default_duration)
            .unwrap_or(chrono::Duration::days(DEFAULT_SESSION_DURATION_DAYS));

        let session = SessionActiveModel {
            id: Set(Uuid::new_v4()),
            user_id: Set(user_id),
            platform: Set(platform.to_string()),
            platform_session_id: Set(platform_session_id.to_string()),
            created_at: Set(now.naive_utc()),
            expires_at: Set(expires_at.naive_utc()),
            metadata: Set(metadata),
        };

        let model = session.insert(&self.db).await?;
        Ok(Session::from(model))
    }

    /// Validate a session by its ID
    pub async fn validate(&self, session_id: Uuid) -> Result<Session, SessionError> {
        let session = SessionEntity::find_by_id(session_id)
            .one(&self.db)
            .await?
            .ok_or(SessionError::NotFound)?;

        let now = Utc::now().naive_utc();
        if session.expires_at < now {
            return Err(SessionError::Expired);
        }

        Ok(Session::from(session))
    }

    /// Validate a session by platform session ID
    pub async fn validate_by_platform_session(
        &self,
        platform: &str,
        platform_session_id: &str,
    ) -> Result<Session, SessionError> {
        let now = Utc::now().naive_utc();
        
        let session = SessionEntity::find()
            .filter(Column::Platform.eq(platform))
            .filter(Column::PlatformSessionId.eq(platform_session_id))
            .filter(Column::ExpiresAt.gt(now))
            .one(&self.db)
            .await?
            .ok_or(SessionError::NotFound)?;

        Ok(Session::from(session))
    }

    /// Get all active sessions for a user
    pub async fn get_user_sessions(&self, user_id: Uuid) -> Result<Vec<Session>, SessionError> {
        let now = Utc::now().naive_utc();
        
        let sessions = SessionEntity::find()
            .filter(Column::UserId.eq(user_id))
            .filter(Column::ExpiresAt.gt(now))
            .all(&self.db)
            .await?;

        Ok(sessions.into_iter().map(Session::from).collect())
    }

    /// Refresh a session (extend expiry)
    pub async fn refresh(&self, session_id: Uuid) -> Result<Session, SessionError> {
        let session = SessionEntity::find_by_id(session_id)
            .one(&self.db)
            .await?
            .ok_or(SessionError::NotFound)?;

        let now = Utc::now().naive_utc();
        if session.expires_at < now {
            return Err(SessionError::Expired);
        }

        let new_expires_at = Utc::now() + chrono::Duration::from_std(self.default_duration)
            .unwrap_or(chrono::Duration::days(DEFAULT_SESSION_DURATION_DAYS));

        let mut active_model: SessionActiveModel = session.into();
        active_model.expires_at = Set(new_expires_at.naive_utc());

        let updated = active_model.update(&self.db).await?;
        Ok(Session::from(updated))
    }

    /// Revoke (delete) a specific session
    pub async fn revoke(&self, session_id: Uuid) -> Result<(), SessionError> {
        let result = SessionEntity::delete_by_id(session_id)
            .exec(&self.db)
            .await?;

        if result.rows_affected == 0 {
            return Err(SessionError::NotFound);
        }

        Ok(())
    }

    /// Revoke all sessions for a user on a specific platform
    pub async fn revoke_platform_sessions(
        &self,
        user_id: Uuid,
        platform: &str,
    ) -> Result<u64, SessionError> {
        let result = SessionEntity::delete_many()
            .filter(Column::UserId.eq(user_id))
            .filter(Column::Platform.eq(platform))
            .exec(&self.db)
            .await?;

        Ok(result.rows_affected)
    }

    /// Revoke all sessions for a user
    pub async fn revoke_all_user_sessions(&self, user_id: Uuid) -> Result<u64, SessionError> {
        let result = SessionEntity::delete_many()
            .filter(Column::UserId.eq(user_id))
            .exec(&self.db)
            .await?;

        Ok(result.rows_affected)
    }

    /// Clean up expired sessions
    pub async fn cleanup_expired(&self) -> Result<u64, SessionError> {
        let now = Utc::now().naive_utc();
        
        let result = SessionEntity::delete_many()
            .filter(Column::ExpiresAt.lt(now))
            .exec(&self.db)
            .await?;

        Ok(result.rows_affected)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Tests would require a test database
}
