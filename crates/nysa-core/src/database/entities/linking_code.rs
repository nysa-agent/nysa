use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "linking_codes")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,
    pub code_hash: String,
    pub user_id: Uuid,
    pub platform: String,
    pub created_at: DateTime,
    pub expires_at: DateTime,
    pub used_at: Option<DateTime>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
