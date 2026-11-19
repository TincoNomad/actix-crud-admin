//! [`SchemaSource`] over SQLite, powered by SQLx.
//!
//! Enabled with `features = ["sqlite"]`.

use async_trait::async_trait;
use serde_json::Value;
use sqlx::sqlite::{SqlitePool, SqlitePoolOptions};
use sqlx::Row as _;

use crate::schema::{
    ColumnKind, ColumnSchema, ForeignKey, RowMap, SchemaCache, SchemaSource, TableSchema,
};
use crate::types::{AdminError, ListQuery, ListResult};

/// Reads tables directly from a SQLite database.
pub struct SqliteSource {
    pool: SqlitePool,
    cache: SchemaCache,
}

/// Rows offered in a foreign key select before it gets unwieldy.
const MAX_OPTIONS: i64 = 500;

impl SqliteSource {
    /// First text, non-primary column; falls back to the value column.
    async fn label_column(&self, table: &str, value_column: &str) -> Result<String, AdminError> {
        let schema = self.describe(table).await?;
        Ok(schema
            .columns
            .iter()
            .find(|c| c.kind == ColumnKind::Text && !c.primary_key)
            .map(|c| c.name.clone())
            .unwrap_or_else(|| value_column.to_string()))
    }

    /// `id -> label` for the given foreign key values.
    async fn labels_for(
        &self,
        table: &str,
        value_column: &str,
        ids: &[String],
    ) -> Result<std::collections::HashMap<String, String>, AdminError> {
        let label = self.label_column(table, value_column).await?;
        let placeholders = vec!["?"; ids.len()].join(", ");
        let sql = format!(
            "SELECT CAST({} AS TEXT) AS v, CAST({} AS TEXT) AS l FROM {} WHERE CAST({} AS TEXT) IN ({})",
            quote(value_column),
            quote(&label),
            quote(table),
            quote(value_column),
            placeholders
        );
        let mut query = sqlx::query(&sql);
        for id in ids {
            query = query.bind(id);
        }
        let rows = query.fetch_all(&self.pool).await.map_err(db_err)?;
        Ok(rows
            .iter()
            .map(|row| {
                (
                    row.get::<Option<String>, _>("v").unwrap_or_default(),
                    row.get::<Option<String>, _>("l").unwrap_or_default(),
                )
            })
            .collect())
    }

    /// Open (and create if missing) a SQLite database.
    pub async fn connect(url: &str) -> Result<Self, AdminError> {
        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect(url)
            .await
            .map_err(db_err)?;
        Ok(Self {
            pool,
            cache: SchemaCache::default(),
        })
    }

    /// Reuse an existing pool.
    pub fn from_pool(pool: SqlitePool) -> Self {
        Self {
            pool,
            cache: SchemaCache::default(),
        }
    }

    /// The underlying pool, for migrations or custom queries.
    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }

    /// Drops the cached schemas, e.g. after adding a column.
    pub fn clear_cache(&self) {
        self.cache.clear();
    }
}

#[async_trait]
impl SchemaSource for SqliteSource {
    async fn describe(&self, table: &str) -> Result<TableSchema, AdminError> {
        if let Some(cached) = self.cache.get(table) {
            return Ok(cached);
        }
        let pragma = format!("PRAGMA table_info({})", quote(table));
        let info = sqlx::query(&pragma)
            .fetch_all(&self.pool)
            .await
            .map_err(db_err)?;
        if info.is_empty() {
            return Err(AdminError::NotFound);
        }

        let fk_pragma = format!("PRAGMA foreign_key_list({})", quote(table));
        let fks = sqlx::query(&fk_pragma)
            .fetch_all(&self.pool)
            .await
            .map_err(db_err)?;

        let columns = info
            .iter()
            .map(|row| {
                let name: String = row.get("name");
                let type_name: String = row.get("type");
                let foreign_key = fks.iter().find_map(|fk| {
                    let from: String = fk.get("from");
                    if from != name {
                        return None;
                    }
                    let target: String = fk.get("table");
                    let column: String = fk.get("to");
                    Some(ForeignKey {
                        table: target,
                        column,
                    })
                });
                ColumnSchema {
                    kind: ColumnKind::from_type_name(&type_name),
                    sql_type: type_name.clone(),
                    nullable: row.get::<i64, _>("notnull") == 0,
                    primary_key: row.get::<i64, _>("pk") > 0,
                    has_default: row.get::<Option<String>, _>("dflt_value").is_some(),
                    name,
                    foreign_key,
                }
            })
            .collect();

        let schema = TableSchema {
            name: table.to_string(),
            columns,
        };
        self.cache.insert(table, &schema);
        Ok(schema)
    }

    async fn rows(&self, table: &str, query: ListQuery) -> Result<ListResult, AdminError> {
        let schema = self.describe(table).await?;
        let per_page = query.per_page.unwrap_or(10).clamp(1, 500);
        let page = query.page.unwrap_or(1).max(1);

        let mut where_sql = String::new();
        let mut search_binds: Vec<String> = Vec::new();
        if let Some(term) = query
            .search
            .as_ref()
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
        {
            let text_columns: Vec<&ColumnSchema> = schema
                .columns
                .iter()
                .filter(|c| c.kind == ColumnKind::Text)
                .collect();
            if !text_columns.is_empty() {
                let clauses: Vec<String> = text_columns
                    .iter()
                    .map(|c| format!("CAST({} AS TEXT) LIKE ?", quote(&c.name)))
                    .collect();
                where_sql = format!(" WHERE ({})", clauses.join(" OR "));
                search_binds = text_columns.iter().map(|_| format!("%{}%", term)).collect();
            }
        }

        let mut filter_binds: Vec<String> = Vec::new();
        for (column, value) in &query.filters {
            let Some(col) = schema.column(column) else {
                continue;
            };
            let connector = if where_sql.is_empty() {
                " WHERE"
            } else {
                " AND"
            };
            where_sql.push_str(&format!(
                "{} CAST({} AS TEXT) = ?",
                connector,
                quote(&col.name)
            ));
            filter_binds.push(value.clone());
        }

        let count_sql = format!("SELECT COUNT(*) FROM {}{}", quote(table), where_sql);
        let mut count_query = sqlx::query(&count_sql);
        for bind in &search_binds {
            count_query = count_query.bind(bind);
        }
        for bind in &filter_binds {
            count_query = count_query.bind(bind);
        }
        let total: i64 = count_query
            .fetch_one(&self.pool)
            .await
            .map_err(db_err)?
            .get(0);

        let order_sql = order_clause(&schema, &query, table);
        let select_sql = format!(
            "SELECT {} FROM {}{} {} LIMIT ? OFFSET ?",
            cast_columns(&schema),
            quote(table),
            where_sql,
            order_sql
        );
        let mut select_query = sqlx::query(&select_sql);
        for bind in &search_binds {
            select_query = select_query.bind(bind);
        }
        for bind in &filter_binds {
            select_query = select_query.bind(bind);
        }
        let offset = (page - 1) * per_page;
        select_query = select_query.bind(per_page as i64).bind(offset as i64);

        let raw_rows = select_query.fetch_all(&self.pool).await.map_err(db_err)?;
        let mut rows: Vec<Value> = raw_rows.iter().map(|row| to_values(&schema, row)).collect();
        for col in schema.columns.iter().filter(|c| c.foreign_key.is_some()) {
            let fk = col.foreign_key.as_ref().expect("checked above");
            let ids: Vec<String> = rows
                .iter()
                .filter_map(|row| row[&col.name].as_str().map(str::to_string))
                .collect();
            if ids.is_empty() {
                continue;
            }
            let labels = self.labels_for(&fk.table, &fk.column, &ids).await?;
            for row in rows.iter_mut() {
                if let Some(id) = row[&col.name].as_str() {
                    if let Some(label) = labels.get(id) {
                        row[&col.name] = Value::String(label.clone());
                    }
                }
            }
        }

        Ok(ListResult {
            rows,
            total: total as u64,
            page,
            per_page,
        })
    }

    async fn row(&self, table: &str, id: &str) -> Result<Value, AdminError> {
        let schema = self.describe(table).await?;
        let pk = schema.primary_key().ok_or(AdminError::NotFound)?;
        let sql = format!(
            "SELECT {} FROM {} WHERE {} = ?",
            cast_columns(&schema),
            quote(table),
            quote(&pk.name)
        );
        let row = sqlx::query(&sql)
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(db_err)?
            .ok_or(AdminError::NotFound)?;
        Ok(to_values(&schema, &row))
    }

    async fn insert(&self, table: &str, data: RowMap) -> Result<Value, AdminError> {
        let schema = self.describe(table).await?;
        if data.is_empty() {
            return Err(AdminError::DatabaseError("no values to insert".into()));
        }
        let entries: Vec<(&String, &Value)> = data.iter().collect();
        let names: Vec<String> = entries.iter().map(|(k, _)| quote(k)).collect();
        let placeholders = vec!["?"; entries.len()].join(", ");
        let sql = format!(
            "INSERT INTO {} ({}) VALUES ({})",
            quote(table),
            names.join(", "),
            placeholders
        );
        let mut query = sqlx::query(&sql);
        for (_, value) in &entries {
            query = bind_value(query, value);
        }
        let result = query.execute(&self.pool).await.map_err(db_err)?;

        let generated = match schema.primary_key() {
            Some(pk) if pk.kind == ColumnKind::Number => {
                Some(result.last_insert_rowid().to_string())
            }
            _ => None,
        };
        Ok(match generated {
            Some(id) => serde_json::json!({ "id": id }),
            None => serde_json::json!({}),
        })
    }

    async fn update(&self, table: &str, id: &str, data: RowMap) -> Result<Value, AdminError> {
        let schema = self.describe(table).await?;
        let pk = schema.primary_key().ok_or(AdminError::NotFound)?;
        if data.is_empty() {
            return Ok(serde_json::json!({ "id": id }));
        }
        let entries: Vec<(&String, &Value)> = data.iter().collect();
        let assignments: Vec<String> = entries
            .iter()
            .map(|(k, _)| format!("{} = ?", quote(k)))
            .collect();
        let sql = format!(
            "UPDATE {} SET {} WHERE {} = ?",
            quote(table),
            assignments.join(", "),
            quote(&pk.name)
        );
        let mut query = sqlx::query(&sql);
        for (_, value) in &entries {
            query = bind_value(query, value);
        }
        let result = query.bind(id).execute(&self.pool).await.map_err(db_err)?;
        if result.rows_affected() == 0 {
            return Err(AdminError::NotFound);
        }
        Ok(serde_json::json!({ "id": id }))
    }

    async fn delete(&self, table: &str, id: &str) -> Result<(), AdminError> {
        let schema = self.describe(table).await?;
        let pk = schema.primary_key().ok_or(AdminError::NotFound)?;
        let sql = format!("DELETE FROM {} WHERE {} = ?", quote(table), quote(&pk.name));
        let result = sqlx::query(&sql)
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(db_err)?;
        if result.rows_affected() == 0 {
            return Err(AdminError::NotFound);
        }
        Ok(())
    }

    async fn options(
        &self,
        table: &str,
        value_column: &str,
    ) -> Result<Vec<(String, String)>, AdminError> {
        let label = self.label_column(table, value_column).await?;
        let sql = format!(
            "SELECT CAST({} AS TEXT) AS v, CAST({} AS TEXT) AS l FROM {} ORDER BY 2 LIMIT ?",
            quote(value_column),
            quote(&label),
            quote(table)
        );
        let rows = sqlx::query(&sql)
            .bind(MAX_OPTIONS)
            .fetch_all(&self.pool)
            .await
            .map_err(db_err)?;
        Ok(rows
            .iter()
            .map(|row| {
                (
                    row.get::<Option<String>, _>("v").unwrap_or_default(),
                    row.get::<Option<String>, _>("l").unwrap_or_default(),
                )
            })
            .collect())
    }
}

fn quote(identifier: &str) -> String {
    format!("\"{}\"", identifier.replace('"', "\"\""))
}

fn cast_columns(schema: &TableSchema) -> String {
    schema
        .columns
        .iter()
        .map(|c| format!("CAST({} AS TEXT) AS {}", quote(&c.name), quote(&c.name)))
        .collect::<Vec<_>>()
        .join(", ")
}

/// The table name is required: the SELECT list aliases every column to the CASTed
/// text value, so a bare column name in ORDER BY would sort as text.
fn order_clause(schema: &TableSchema, query: &ListQuery, table: &str) -> String {
    let column = query.sort_by.as_ref().and_then(|name| schema.column(name));
    match column {
        Some(col) => {
            let dir = match query.sort_dir.as_deref() {
                Some(d) if d.eq_ignore_ascii_case("desc") => "DESC",
                _ => "ASC",
            };
            format!("ORDER BY {}.{} {}", quote(table), quote(&col.name), dir)
        }
        None => match schema.primary_key() {
            Some(pk) if pk.kind == ColumnKind::Number => {
                format!("ORDER BY {}.{} DESC", quote(table), quote(&pk.name))
            }
            Some(pk) => format!("ORDER BY {}.{} ASC", quote(table), quote(&pk.name)),
            None => String::new(),
        },
    }
}

fn to_values(schema: &TableSchema, row: &sqlx::sqlite::SqliteRow) -> serde_json::Value {
    let mut map = serde_json::Map::new();
    for col in &schema.columns {
        let raw = row
            .try_get::<Option<String>, _>(col.name.as_str())
            .ok()
            .flatten();
        let value = match raw {
            None => Value::Null,
            Some(text) if col.kind == ColumnKind::Boolean => {
                Value::Bool(matches!(text.as_str(), "1" | "t" | "T" | "true" | "TRUE"))
            }
            Some(text) => Value::String(text),
        };
        map.insert(col.name.clone(), value);
    }
    Value::Object(map)
}

fn bind_value<'q>(
    query: sqlx::query::Query<'q, sqlx::Sqlite, sqlx::sqlite::SqliteArguments<'q>>,
    value: &'q Value,
) -> sqlx::query::Query<'q, sqlx::Sqlite, sqlx::sqlite::SqliteArguments<'q>> {
    match value {
        Value::Null => query.bind(None::<String>),
        Value::Bool(b) => query.bind(i64::from(*b)),
        Value::Number(n) => query.bind(n.to_string()),
        Value::String(s) => query.bind(s.as_str()),
        other => query.bind(other.to_string()),
    }
}

fn db_err(error: sqlx::Error) -> AdminError {
    AdminError::DatabaseError(error.to_string())
}
