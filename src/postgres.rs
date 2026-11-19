//! [`SchemaSource`] over PostgreSQL, powered by SQLx.
//!
//! Enabled with `features = ["postgres"]`. Works with Supabase: use the
//! session pooler (port 5432) and `?sslmode=require`.

use async_trait::async_trait;
use serde_json::Value;
use sqlx::postgres::{PgConnectOptions, PgPool, PgPoolOptions};
use sqlx::Row as _;
use std::str::FromStr;

use crate::schema::{ColumnKind, ColumnSchema, ForeignKey, RowMap, SchemaSource, TableSchema};
use crate::types::{AdminError, ListQuery, ListResult};

const MAX_OPTIONS: i64 = 500;
/// Schema the admin reads. Multi-schema support is not implemented.
const SCHEMA: &str = "public";

/// Reads tables directly from a PostgreSQL database.
pub struct PostgresSource {
    pool: PgPool,
}

impl PostgresSource {
    /// Open a pool. Keep the session pooler (port 5432) when using Supabase.
    pub async fn connect(url: &str) -> Result<Self, AdminError> {
        // No prepared statement cache: safe behind a transaction pooler.
        let options = PgConnectOptions::from_str(url)
            .map_err(db_err)?
            .statement_cache_capacity(0);
        let pool = PgPoolOptions::new()
            .max_connections(5)
            .connect_with(options)
            .await
            .map_err(db_err)?;
        Ok(Self { pool })
    }

    /// Reuse an existing pool.
    pub fn from_pool(pool: PgPool) -> Self {
        Self { pool }
    }

    /// The underlying pool, for migrations or custom queries.
    pub fn pool(&self) -> &PgPool {
        &self.pool
    }
}

#[async_trait]
impl SchemaSource for PostgresSource {
    async fn describe(&self, table: &str) -> Result<TableSchema, AdminError> {
        let columns = sqlx::query(
            "SELECT column_name, data_type, is_nullable, column_default
             FROM information_schema.columns
             WHERE table_schema = $1 AND table_name = $2
             ORDER BY ordinal_position",
        )
        .bind(SCHEMA)
        .bind(table)
        .fetch_all(&self.pool)
        .await
        .map_err(db_err)?;
        if columns.is_empty() {
            return Err(AdminError::NotFound);
        }

        let primary_keys: Vec<String> = sqlx::query_scalar(
            "SELECT kcu.column_name
             FROM information_schema.table_constraints tc
             JOIN information_schema.key_column_usage kcu
               ON kcu.constraint_name = tc.constraint_name
              AND kcu.table_schema = tc.table_schema
             WHERE tc.constraint_type = 'PRIMARY KEY'
               AND tc.table_schema = $1 AND tc.table_name = $2",
        )
        .bind(SCHEMA)
        .bind(table)
        .fetch_all(&self.pool)
        .await
        .map_err(db_err)?;

        let foreign_keys = sqlx::query(
            "SELECT kcu.column_name AS column_name,
                    ccu.table_name  AS foreign_table,
                    ccu.column_name AS foreign_column
             FROM information_schema.table_constraints tc
             JOIN information_schema.key_column_usage kcu
               ON kcu.constraint_name = tc.constraint_name
              AND kcu.table_schema = tc.table_schema
             JOIN information_schema.constraint_column_usage ccu
               ON ccu.constraint_name = tc.constraint_name
              AND ccu.table_schema = tc.table_schema
             WHERE tc.constraint_type = 'FOREIGN KEY'
               AND tc.table_schema = $1 AND tc.table_name = $2",
        )
        .bind(SCHEMA)
        .bind(table)
        .fetch_all(&self.pool)
        .await
        .map_err(db_err)?;

        let columns = columns
            .iter()
            .map(|row| {
                let name: String = row.get("column_name");
                let type_name: String = row.get("data_type");
                let foreign_key = foreign_keys.iter().find_map(|fk| {
                    let column: String = fk.get("column_name");
                    if column != name {
                        return None;
                    }
                    Some(ForeignKey {
                        table: fk.get("foreign_table"),
                        column: fk.get("foreign_column"),
                    })
                });
                ColumnSchema {
                    kind: ColumnKind::from_type_name(&type_name),
                    sql_type: type_name,
                    nullable: row.get::<String, _>("is_nullable") == "YES",
                    primary_key: primary_keys.contains(&name),
                    has_default: row
                        .try_get::<Option<String>, _>("column_default")
                        .ok()
                        .flatten()
                        .is_some(),
                    name,
                    foreign_key,
                }
            })
            .collect();

        Ok(TableSchema {
            name: table.to_string(),
            columns,
        })
    }

    async fn rows(&self, table: &str, query: ListQuery) -> Result<ListResult, AdminError> {
        let schema = self.describe(table).await?;
        let per_page = query.per_page.unwrap_or(10).clamp(1, 500);
        let page = query.page.unwrap_or(1).max(1);
        let mut params = Params::default();

        let mut where_sql = String::new();
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
                    .map(|c| {
                        let slot = params.push(Some(format!("%{}%", term)));
                        format!("CAST({} AS TEXT) ILIKE {}", quote(&c.name), slot)
                    })
                    .collect();
                where_sql = format!(" WHERE ({})", clauses.join(" OR "));
            }
        }

        for (column, value) in &query.filters {
            let Some(col) = schema.column(column) else {
                continue;
            };
            let connector = if where_sql.is_empty() {
                " WHERE"
            } else {
                " AND"
            };
            let slot = params.push(Some(value.clone()));
            where_sql.push_str(&format!(
                "{} CAST({} AS TEXT) = {}",
                connector,
                quote(&col.name),
                slot
            ));
        }

        let count_sql = format!("SELECT COUNT(*) FROM {}{}", quote(table), where_sql);
        let mut count_query = sqlx::query_scalar::<_, i64>(&count_sql);
        for value in &params.values {
            count_query = count_query.bind(value.as_deref());
        }
        let total: i64 = count_query.fetch_one(&self.pool).await.map_err(db_err)?;

        // Inlined because Postgres will not accept a text bind for LIMIT/OFFSET.
        let offset = (page - 1).saturating_mul(per_page);
        let select_sql = format!(
            "SELECT {} FROM {}{} {} LIMIT {} OFFSET {}",
            cast_columns(&schema),
            quote(table),
            where_sql,
            order_clause(&schema, &query),
            per_page,
            offset
        );

        let mut select_query = sqlx::query(&select_sql);
        for value in &params.values {
            select_query = select_query.bind(value.as_deref());
        }
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
            "SELECT {} FROM {} WHERE CAST({} AS TEXT) = $1",
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
        let mut params = Params::default();
        let mut names = Vec::new();
        let mut placeholders = Vec::new();
        for (column, value) in data.iter() {
            let Some(col) = schema.column(column) else {
                continue;
            };
            names.push(quote(&col.name));
            placeholders.push(format!(
                "CAST({} AS {})",
                params.push(as_param(value)),
                col.sql_type
            ));
        }
        if names.is_empty() {
            return Err(AdminError::DatabaseError(
                "no known columns to insert".into(),
            ));
        }

        let returning = match schema.primary_key() {
            Some(pk) => format!(" RETURNING CAST({} AS TEXT)", quote(&pk.name)),
            None => String::new(),
        };
        let sql = format!(
            "INSERT INTO {} ({}) VALUES ({}){}",
            quote(table),
            names.join(", "),
            placeholders.join(", "),
            returning
        );
        let mut query = sqlx::query(&sql);
        for value in &params.values {
            query = query.bind(value.as_deref());
        }

        if returning.is_empty() {
            query.execute(&self.pool).await.map_err(db_err)?;
            return Ok(serde_json::json!({}));
        }
        let id: Option<String> = query
            .fetch_one(&self.pool)
            .await
            .map_err(db_err)?
            .try_get(0)
            .ok();
        Ok(match id {
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
        let mut params = Params::default();
        let mut assignments = Vec::new();
        for (column, value) in data.iter() {
            let Some(col) = schema.column(column) else {
                continue;
            };
            if col.primary_key {
                continue;
            }
            assignments.push(format!(
                "{} = CAST({} AS {})",
                quote(&col.name),
                params.push(as_param(value)),
                col.sql_type
            ));
        }
        if assignments.is_empty() {
            return Ok(serde_json::json!({ "id": id }));
        }
        let id_slot = params.push(Some(id.to_string()));
        let sql = format!(
            "UPDATE {} SET {} WHERE CAST({} AS TEXT) = {}",
            quote(table),
            assignments.join(", "),
            quote(&pk.name),
            id_slot
        );
        let mut query = sqlx::query(&sql);
        for value in &params.values {
            query = query.bind(value.as_deref());
        }
        let result = query.execute(&self.pool).await.map_err(db_err)?;
        if result.rows_affected() == 0 {
            return Err(AdminError::NotFound);
        }
        Ok(serde_json::json!({ "id": id }))
    }

    async fn delete(&self, table: &str, id: &str) -> Result<(), AdminError> {
        let schema = self.describe(table).await?;
        let pk = schema.primary_key().ok_or(AdminError::NotFound)?;
        let sql = format!(
            "DELETE FROM {} WHERE CAST({} AS TEXT) = $1",
            quote(table),
            quote(&pk.name)
        );
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
            "SELECT CAST({} AS TEXT) AS v, CAST({} AS TEXT) AS l FROM {} ORDER BY 2 LIMIT $1",
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

impl PostgresSource {
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
        let placeholders: Vec<String> = (1..=ids.len()).map(|n| format!("${}", n)).collect();
        let sql = format!(
            "SELECT CAST({} AS TEXT) AS v, CAST({} AS TEXT) AS l
             FROM {} WHERE CAST({} AS TEXT) IN ({})",
            quote(value_column),
            quote(&label),
            quote(table),
            quote(value_column),
            placeholders.join(", ")
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
}

/// Accumulates bind values and hands out `$1..$n` placeholders in order.
#[derive(Default)]
struct Params {
    values: Vec<Option<String>>,
    used: usize,
}

impl Params {
    fn push(&mut self, value: Option<String>) -> String {
        self.values.push(value);
        self.used += 1;
        format!("${}", self.used)
    }
}

/// Every bind goes as text; the SQL wraps it in `CAST(... AS <type>)`.
fn as_param(value: &Value) -> Option<String> {
    match value {
        Value::Null => None,
        Value::Bool(b) => Some(if *b { "true".into() } else { "false".into() }),
        Value::String(s) => Some(s.clone()),
        other => Some(other.to_string()),
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

fn order_clause(schema: &TableSchema, query: &ListQuery) -> String {
    let column = query.sort_by.as_ref().and_then(|name| schema.column(name));
    match column {
        Some(col) => {
            let dir = match query.sort_dir.as_deref() {
                Some(d) if d.eq_ignore_ascii_case("desc") => "DESC",
                _ => "ASC",
            };
            format!("ORDER BY {} {}", quote(&col.name), dir)
        }
        None => match schema.primary_key() {
            Some(pk) => format!("ORDER BY {} DESC", quote(&pk.name)),
            None => String::new(),
        },
    }
}

fn to_values(schema: &TableSchema, row: &sqlx::postgres::PgRow) -> Value {
    let mut map = serde_json::Map::new();
    for col in &schema.columns {
        let raw = row
            .try_get::<Option<String>, _>(col.name.as_str())
            .ok()
            .flatten();
        let value = match raw {
            None => Value::Null,
            Some(text) if col.kind == ColumnKind::Boolean => Value::Bool(text == "true"),
            Some(text) => Value::String(text),
        };
        map.insert(col.name.clone(), value);
    }
    Value::Object(map)
}

fn db_err(error: sqlx::Error) -> AdminError {
    AdminError::DatabaseError(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn placeholders_are_numbered_in_order() {
        let mut params = Params::default();
        assert_eq!(params.push(Some("a".into())), "$1");
        assert_eq!(params.push(None), "$2");
        assert_eq!(params.push(Some("c".into())), "$3");
        assert_eq!(params.values.len(), 3);
    }

    #[test]
    fn booleans_bind_as_text() {
        assert_eq!(as_param(&Value::Bool(true)), Some("true".into()));
        assert_eq!(as_param(&Value::Bool(false)), Some("false".into()));
        assert_eq!(as_param(&Value::Null), None);
        assert_eq!(as_param(&Value::String("x".into())), Some("x".into()));
    }

    #[test]
    fn identifiers_are_double_quoted() {
        assert_eq!(quote("name"), "\"name\"");
        assert_eq!(quote("we\"ird"), "\"we\"\"ird\"");
    }

    #[test]
    fn unknown_sort_columns_fall_back_to_the_primary_key() {
        let schema = TableSchema {
            name: "t".into(),
            columns: vec![ColumnSchema {
                name: "id".into(),
                kind: ColumnKind::Number,
                sql_type: "integer".into(),
                nullable: false,
                primary_key: true,
                has_default: false,
                foreign_key: None,
            }],
        };
        let query = ListQuery {
            sort_by: Some("name\"; DROP TABLE x; --".into()),
            ..Default::default()
        };
        assert_eq!(order_clause(&schema, &query), "ORDER BY \"id\" DESC");
    }
}
