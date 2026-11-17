pub mod product;

use std::sync::Arc;
use actix_crud_admin::registry::AdminRegistry;
use crate::db::DbPool;
use crate::admin::product::ProductAdmin;

pub fn build_registry(pool: Arc<DbPool>) -> AdminRegistry {
    let mut registry = AdminRegistry::new();
    registry.register(ProductAdmin::new(pool));
    registry
}
