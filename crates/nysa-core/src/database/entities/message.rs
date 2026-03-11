use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "messages")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: Uuid,
    pub thread_id: Uuid,
    pub platform_message_id: Option<i64>,
    pub author_internal_id: Option<Uuid>,
    pub author_platform_id: Option<String>,
    pub author_name: String,
    pub content: String,
    pub role: String,
    pub created_at: DateTime,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
