use chrono::Utc;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, NotSet, QueryFilter, Set,
};
use std::time::Duration;
use thiserror::Error;
use uuid::Uuid;

use crate::auth::{TokenError, generate_linking_code, hash_linking_code, verify_linking_code};
use crate::database::entities::linking_code::Entity as LinkingCodeEntity;
use crate::database::entities::linking_code::{ActiveModel as LinkingCodeActiveModel, Column};

const LINKING_CODE_EXPIRY_MINUTES: i64 = 5;

#[derive(Debug, Error)]
pub enum LinkingCodeError {
    #[error("Database error: {0}")]
    Database(#[from] sea_orm::DbErr),
    #[error("Invalid linking code")]
    InvalidCode,
    #[error("Linking code expired")]
    Expired,
    #[error("Linking code already used")]
    AlreadyUsed,
    #[error("Platform already linked to this user")]
    PlatformAlreadyLinked,
    #[error("Token error: {0}")]
    TokenError(#[from] TokenError),
}

pub struct LinkingCodeService {
    db: DatabaseConnection,
}

impl LinkingCodeService {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }

    /// Generate a new linking code for a user
    /// Returns the plain code (must be shown to user) and stores the hash
    pub async fn generate_code(
        &self,
        user_id: Uuid,
        platform: &str,
    ) -> Result<String, LinkingCodeError> {
        // Check if platform is already linked
        let user = crate::database::entities::user::Entity::find_by_id(user_id)
            .one(&self.db)
            .await?;

        if let Some(user) = user
            && let Some(profiles) = user.linked_profiles.as_object()
            && profiles.contains_key(platform)
        {
            return Err(LinkingCodeError::PlatformAlreadyLinked);
        }

        // Generate a unique code
        let plain_code = loop {
            let code = generate_linking_code();
            let code_hash = hash_linking_code(&code)?;

            // Check if this hash already exists
            let existing = LinkingCodeEntity::find()
                .filter(Column::CodeHash.eq(&code_hash))
                .filter(Column::UsedAt.is_null())
                .filter(Column::ExpiresAt.gt(Utc::now().naive_utc()))
                .one(&self.db)
                .await?;

            if existing.is_none() {
                // Create the linking code record
                let expires_at =
                    Utc::now() + Duration::from_secs(LINKING_CODE_EXPIRY_MINUTES as u64 * 60);

                let linking_code = LinkingCodeActiveModel {
                    id: NotSet,
                    code_hash: Set(code_hash),
                    user_id: Set(user_id),
                    platform: Set(platform.to_string()),
                    created_at: Set(Utc::now().naive_utc()),
                    expires_at: Set(expires_at.naive_utc()),
                    used_at: Set(None),
                };

                linking_code.insert(&self.db).await?;
                break code;
            }
        };

        Ok(plain_code)
    }

    /// Redeem a linking code to link a platform profile to a user
    /// Returns the user_id on success
    pub async fn redeem_code(
        &self,
        code: &str,
        platform: &str,
        platform_id: &str,
        metadata: serde_json::Value,
    ) -> Result<Uuid, LinkingCodeError> {
        // Find all unused, non-expired codes for this platform
        let candidates = LinkingCodeEntity::find()
            .filter(Column::Platform.eq(platform))
            .filter(Column::UsedAt.is_null())
            .filter(Column::ExpiresAt.gt(Utc::now().naive_utc()))
            .all(&self.db)
            .await?;

        // Find matching code by verifying hash
        let matching_code = candidates
            .into_iter()
            .find(|c| verify_linking_code(code, &c.code_hash));

        let linking_code = match matching_code {
            Some(c) => c,
            None => return Err(LinkingCodeError::InvalidCode),
        };

        // Mark as used
        let mut active_model: LinkingCodeActiveModel = linking_code.clone().into();
        active_model.used_at = Set(Some(Utc::now().naive_utc()));
        active_model.update(&self.db).await?;

        // Link the platform profile to the user
        let user_id = linking_code.user_id;

        let user = crate::database::entities::user::Entity::find_by_id(user_id)
            .one(&self.db)
            .await?;

        if let Some(user) = user {
            let mut profiles = user.linked_profiles.clone();

            if let Some(obj) = profiles.as_object_mut() {
                obj.insert(
                    platform.to_string(),
                    serde_json::json!({
                        "id": platform_id,
                        "metadata": metadata,
                        "linked_at": Utc::now().to_rfc3339(),
                    }),
                );
            }

            let mut active_user: crate::database::entities::user::ActiveModel = user.into();
            active_user.linked_profiles = Set(profiles);
            active_user.update(&self.db).await?;
        }

        Ok(user_id)
    }

    /// Get user ID by linking code (for validation without redeeming)
    pub async fn get_user_by_code(&self, code: &str) -> Result<Option<Uuid>, LinkingCodeError> {
        let candidates = LinkingCodeEntity::find()
            .filter(Column::UsedAt.is_null())
            .filter(Column::ExpiresAt.gt(Utc::now().naive_utc()))
            .all(&self.db)
            .await?;

        let matching = candidates
            .into_iter()
            .find(|c| verify_linking_code(code, &c.code_hash));

        Ok(matching.map(|c| c.user_id))
    }

    /// Clean up expired linking codes
    pub async fn cleanup_expired(&self) -> Result<u64, LinkingCodeError> {
        let result = LinkingCodeEntity::delete_many()
            .filter(Column::ExpiresAt.lt(Utc::now().naive_utc()))
            .exec(&self.db)
            .await?;

        Ok(result.rows_affected)
    }
}

#[cfg(test)]
mod tests {
    // Note: These tests would need a test database to run
    // They're placeholders for the actual implementation structure

    #[test]
    fn test_linking_code_service_new() {
        // Just verify the service can be created
        // Real tests would use a test database connection
    }
}
