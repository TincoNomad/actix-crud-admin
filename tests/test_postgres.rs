//! Integration tests against a real PostgreSQL database.
//!
//! They only run when `TEST_POSTGRES_URL` (or `POSTGRES_URL`) is set:
//!
//! ```bash
//! cargo test --features postgres --test test_postgres
//! ```

#![cfg(feature = "postgres")]

use actix_crud_admin::generic::GenericTable;
use actix_crud_admin::postgres::PostgresSource;
use actix_crud_admin::schema::{ColumnKind, SchemaSource};
use actix_crud_admin::types::*;
use actix_crud_admin::AdminResource;
use serde_json::json;
use std::collections::HashMap;
use std::sync::Arc;

fn database_url() -> Option<String> {
    std::env::var("TEST_POSTGRES_URL")
        .or_else(|_| std::env::var("POSTGRES_URL"))
        .ok()
        .filter(|url| !url.trim().is_empty())
}

async fn source() -> Option<Arc<PostgresSource>> {
    let url = database_url()?;
    Some(Arc::new(
        PostgresSource::connect(&url)
            .await
            .expect("connect to postgres"),
    ))
}

async fn exec(source: &PostgresSource, sql: &str) {
    sqlx::query(sql)
        .execute(source.pool())
        .await
        .unwrap_or_else(|e| panic!("{} failed: {}", sql, e));
}

#[tokio::test]
async fn postgres_end_to_end() {
    let Some(source) = source().await else {
        eprintln!("skipping: TEST_POSTGRES_URL not set");
        return;
    };

    // --- probe schema -----------------------------------------------------
    eprintln!("[1/7] creando tablas de prueba");
    exec(&source, "drop table if exists admin_probe_child").await;
    exec(&source, "drop table if exists admin_probe_parent").await;
    exec(
        &source,
        "create table admin_probe_parent (
            id uuid primary key default gen_random_uuid(),
            name text not null,
            active boolean not null default true
        )",
    )
    .await;
    exec(
        &source,
        "create table admin_probe_child (
            id uuid primary key default gen_random_uuid(),
            name text not null,
            price numeric(10,2) not null default 0,
            active boolean not null default true,
            notes text,
            parent_id uuid references admin_probe_parent(id),
            created_at timestamptz not null default now()
        )",
    )
    .await;

    // --- describe ---------------------------------------------------------
    eprintln!("[2/7] describe");
    let schema = source.describe("admin_probe_child").await.unwrap();
    let names: Vec<&str> = schema.columns.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(
        names,
        vec![
            "id",
            "name",
            "price",
            "active",
            "notes",
            "parent_id",
            "created_at"
        ]
    );
    let id = schema.column("id").unwrap();
    assert!(id.primary_key);
    assert_eq!(id.kind, ColumnKind::Text, "uuid maps to text");
    assert_eq!(id.sql_type, "uuid");
    assert_eq!(schema.column("price").unwrap().kind, ColumnKind::Number);
    assert_eq!(schema.column("active").unwrap().kind, ColumnKind::Boolean);
    assert_eq!(schema.column("created_at").unwrap().kind, ColumnKind::Date);
    assert!(
        schema.column("created_at").unwrap().has_default,
        "now() should be detected as a default"
    );
    let fk = schema
        .column("parent_id")
        .unwrap()
        .foreign_key
        .as_ref()
        .unwrap();
    assert_eq!(fk.table, "admin_probe_parent");
    assert_eq!(fk.column, "id");

    assert!(matches!(
        source.describe("admin_probe_missing").await,
        Err(AdminError::NotFound)
    ));

    // --- form fields ------------------------------------------------------
    eprintln!("[3/7] form fields");
    let child = GenericTable::new(source.clone(), "admin_probe_child")
        .await
        .unwrap();
    let fields = child.form_fields_dynamic().await;
    let keys: Vec<&str> = fields.iter().map(|f| f.key.as_str()).collect();
    assert_eq!(
        keys,
        vec![
            "name",
            "price",
            "active",
            "notes",
            "parent_id",
            "created_at"
        ]
    );
    let required: Vec<&str> = fields
        .iter()
        .filter(|f| f.required)
        .map(|f| f.key.as_str())
        .collect();
    assert_eq!(required, vec!["name"], "only NOT NULL without default");

    // --- create with a generated uuid -------------------------------------
    eprintln!("[4/7] inserts");
    let parent = GenericTable::new(source.clone(), "admin_probe_parent")
        .await
        .unwrap();
    let parent_id = parent
        .create(HashMap::from([("name".to_string(), json!("Hardware"))]))
        .await
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(parent_id.len(), 36, "uuid should be returned as text");

    let created = child
        .create(HashMap::from([
            ("name".to_string(), json!("Tea")),
            ("price".to_string(), json!("9.99")),
            ("active".to_string(), json!("on")),
            ("notes".to_string(), json!("")),
            ("parent_id".to_string(), json!(parent_id)),
        ]))
        .await
        .unwrap();
    let id = created["id"].as_str().unwrap().to_string();

    let row = child.get(&id).await.unwrap();
    assert_eq!(row["name"], json!("Tea"));
    assert_eq!(row["active"], json!(true));
    assert_eq!(row["notes"], json!(null), "empty nullable becomes null");
    assert!(row["created_at"].is_string(), "default was applied");
    assert_eq!(row["parent_id"], json!(parent_id));

    // --- list: search, sort, filter, labels -------------------------------
    eprintln!("[5/7] list");
    let result = child
        .list(ListQuery {
            search: Some("tea".into()),
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(result.total, 1, "search is case insensitive (ILIKE)");

    let result = child
        .list(ListQuery {
            sort_by: Some("price".into()),
            sort_dir: Some("desc".into()),
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(result.rows.len(), 1);

    let result = child
        .list(ListQuery {
            filters: HashMap::from([("parent_id".to_string(), parent_id.clone())]),
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(result.total, 1);
    assert_eq!(
        result.rows[0]["parent_id"],
        json!("Hardware"),
        "the foreign key shows the label, not the uuid"
    );

    let specs = child.filters().await;
    assert_eq!(specs.len(), 1);
    assert_eq!(specs[0].key, "parent_id");

    let options = source.options("admin_probe_parent", "id").await.unwrap();
    assert_eq!(options, vec![(parent_id.clone(), "Hardware".to_string())]);

    // --- pagination -------------------------------------------------------
    eprintln!("[6/7] paginacion");
    for name in ["A", "B", "C"] {
        child
            .create(HashMap::from([("name".to_string(), json!(name))]))
            .await
            .unwrap();
    }
    let page = child
        .list(ListQuery {
            page: Some(2),
            per_page: Some(2),
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(page.total, 4);
    assert_eq!(page.rows.len(), 2);

    // --- update and delete ------------------------------------------------
    eprintln!("[7/7] update/delete");
    child
        .update(
            &id,
            HashMap::from([
                ("name".to_string(), json!("Green tea")),
                ("price".to_string(), json!("12.50")),
            ]),
        )
        .await
        .unwrap();
    let row = child.get(&id).await.unwrap();
    assert_eq!(row["name"], json!("Green tea"));
    assert_eq!(row["price"], json!("12.50"));

    child.delete(&id).await.unwrap();
    assert!(matches!(child.get(&id).await, Err(AdminError::NotFound)));
    assert!(matches!(child.delete(&id).await, Err(AdminError::NotFound)));

    // --- injection guard --------------------------------------------------
    let result = child
        .list(ListQuery {
            sort_by: Some("name\"; drop table admin_probe_child; --".into()),
            ..Default::default()
        })
        .await
        .unwrap();
    assert!(result.total > 0, "table must survive the injection attempt");

    exec(&source, "drop table if exists admin_probe_child").await;
    exec(&source, "drop table if exists admin_probe_parent").await;
}

/// Read-only pass over whatever real tables the database already has.
#[tokio::test]
async fn postgres_reads_the_real_schema() {
    let Some(source) = source().await else {
        eprintln!("skipping: TEST_POSTGRES_URL not set");
        return;
    };

    let Ok(schema) = source.describe("products").await else {
        eprintln!("skipping: no products table in this database");
        return;
    };
    assert!(!schema.columns.is_empty());
    let table = GenericTable::new(source.clone(), "products").await.unwrap();
    let result = table.list(ListQuery::default()).await.unwrap();
    assert!(result.per_page > 0);
    let columns = table.list_columns();
    assert!(!columns.is_empty());
}
