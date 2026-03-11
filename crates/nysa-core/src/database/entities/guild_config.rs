use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "guild_configs")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub guild_id: i64,
    pub default_mode: String,
    pub proactive_range: Json,
    pub dm_mode: String,
    pub config: Json,
    pub created_at: DateTime,
    pub updated_at: DateTime,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
