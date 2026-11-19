//! Schema discovery: turn an existing database table into an admin panel.
//!
//! Implement [`SchemaSource`] once for your database of choice (SQLite, Postgres,
//! Diesel, SQLx, anything) and then register tables with
//! [`GenericTable`](crate::generic::GenericTable).

use async_trait::async_trait;
use std::collections::HashMap;

use crate::types::{AdminError, ListQuery, ListResult};

/// Row shaped as the crate's transport format (strings, bools and nulls).
pub type RowMap = HashMap<String, serde_json::Value>;

/// How a database column maps to the admin UI.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColumnKind {
    Text,
    Number,
    Boolean,
    Date,
    Binary,
}

impl ColumnKind {
    /// Best-effort mapping from a declared SQL type name.
    pub fn from_type_name(type_name: &str) -> Self {
        let t = type_name.to_ascii_lowercase();
        if t.contains("bool") {
            ColumnKind::Boolean
        } else if t.contains("date") || t.contains("time") {
            ColumnKind::Date
        } else if t.contains("int")
            || t.contains("serial")
            || t.contains("real")
            || t.contains("floa")
            || t.contains("doub")
            || t.contains("numeric")
            || t.contains("decimal")
        {
            ColumnKind::Number
        } else if t.contains("blob") || t.contains("binary") || t.contains("bytea") {
            ColumnKind::Binary
        } else {
            ColumnKind::Text
        }
    }
}

/// A column that references another table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForeignKey {
    pub table: String,
    pub column: String,
}

/// One column of a table.
#[derive(Debug, Clone)]
pub struct ColumnSchema {
    pub name: String,
    pub kind: ColumnKind,
    /// Declared SQL type, used to cast bind parameters (e.g. `uuid`, `integer`).
    pub sql_type: String,
    pub nullable: bool,
    pub primary_key: bool,
    /// The column has a SQL default, so the form can leave it empty.
    pub has_default: bool,
    pub foreign_key: Option<ForeignKey>,
}

/// The shape of a table.
#[derive(Debug, Clone)]
pub struct TableSchema {
    pub name: String,
    pub columns: Vec<ColumnSchema>,
}

impl TableSchema {
    pub fn column(&self, name: &str) -> Option<&ColumnSchema> {
        self.columns.iter().find(|c| c.name == name)
    }

    pub fn primary_key(&self) -> Option<&ColumnSchema> {
        self.columns.iter().find(|c| c.primary_key)
    }
}

/// Caches table descriptions: the database round trip is the expensive part.
#[derive(Default)]
pub struct SchemaCache {
    tables: std::sync::Mutex<HashMap<String, TableSchema>>,
}

impl SchemaCache {
    pub fn get(&self, table: &str) -> Option<TableSchema> {
        self.tables.lock().ok()?.get(table).cloned()
    }

    pub fn insert(&self, table: &str, schema: &TableSchema) {
        if let Ok(mut tables) = self.tables.lock() {
            tables.insert(table.to_string(), schema.clone());
        }
    }

    pub fn clear(&self) {
        if let Ok(mut tables) = self.tables.lock() {
            tables.clear();
        }
    }
}

/// Backend that can describe a table and run generic CRUD over it.
///
/// Implement this over your existing pool (Diesel, SQLx, SeaORM) to get an admin
/// for tables that have no hand-written [`AdminResource`](crate::AdminResource).
#[async_trait]
pub trait SchemaSource: Send + Sync + 'static {
    /// Read the columns of a table. Called once per registered table.
    async fn describe(&self, table: &str) -> Result<TableSchema, AdminError>;

    /// Paginated, searchable and sortable listing.
    async fn rows(&self, table: &str, query: ListQuery) -> Result<ListResult, AdminError>;

    /// Fetch one row by primary key value.
    async fn row(&self, table: &str, id: &str) -> Result<serde_json::Value, AdminError>;

    /// Insert a row and return `{"id": <primary key>}`.
    async fn insert(&self, table: &str, data: RowMap) -> Result<serde_json::Value, AdminError>;

    /// Update a row and return `{"id": <primary key>}`.
    async fn update(
        &self,
        table: &str,
        id: &str,
        data: RowMap,
    ) -> Result<serde_json::Value, AdminError>;

    /// Delete a row by primary key value.
    async fn delete(&self, table: &str, id: &str) -> Result<(), AdminError>;

    /// Choices for a foreign key select: `(value, label)`.
    async fn options(
        &self,
        table: &str,
        value_column: &str,
    ) -> Result<Vec<(String, String)>, AdminError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn schema(name: &str) -> TableSchema {
        TableSchema {
            name: name.to_string(),
            columns: Vec::new(),
        }
    }

    #[test]
    fn cache_round_trip_and_clear() {
        let cache = SchemaCache::default();
        assert!(cache.get("products").is_none());
        cache.insert("products", &schema("products"));
        assert_eq!(cache.get("products").unwrap().name, "products");
        cache.clear();
        assert!(cache.get("products").is_none());
    }

    #[test]
    fn column_kind_maps_declared_types() {
        use ColumnKind::*;
        assert_eq!(ColumnKind::from_type_name("integer"), Number);
        assert_eq!(ColumnKind::from_type_name("numeric(10,2)"), Number);
        assert_eq!(ColumnKind::from_type_name("timestamp with time zone"), Date);
        assert_eq!(ColumnKind::from_type_name("boolean"), Boolean);
        assert_eq!(ColumnKind::from_type_name("uuid"), Text);
        assert_eq!(ColumnKind::from_type_name("bytea"), Binary);
    }
}
