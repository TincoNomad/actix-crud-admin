//! Tests for schema-driven resources (`GenericTable`) that do not need a real database.

use actix_crud_admin::generic::GenericTable;
use actix_crud_admin::schema::{
    ColumnKind, ColumnSchema, ForeignKey, RowMap, SchemaSource, TableSchema,
};
use actix_crud_admin::types::*;
use actix_crud_admin::AdminResource;
use async_trait::async_trait;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

fn products_schema() -> TableSchema {
    let column = |name: &str, kind, nullable, primary_key, foreign_key| ColumnSchema {
        name: name.to_string(),
        kind,
        sql_type: "TEXT".into(),
        nullable,
        primary_key,
        has_default: false,
        foreign_key,
    };
    TableSchema {
        name: "products".into(),
        columns: vec![
            column("id", ColumnKind::Number, false, true, None),
            column("name", ColumnKind::Text, false, false, None),
            column("price", ColumnKind::Number, true, false, None),
            column("active", ColumnKind::Boolean, false, false, None),
            column(
                "category_id",
                ColumnKind::Number,
                true,
                false,
                Some(ForeignKey {
                    table: "categories".into(),
                    column: "id".into(),
                }),
            ),
        ],
    }
}

struct MemorySource {
    schema: TableSchema,
    rows: Arc<Mutex<Vec<RowMap>>>,
}

impl MemorySource {
    fn new() -> Self {
        let rows: Vec<RowMap> = vec![
            json!({"id": "1", "name": "Tea", "price": "3.5", "active": true, "category_id": "1"})
                .as_object()
                .unwrap()
                .clone()
                .into_iter()
                .collect(),
        ];
        Self {
            schema: products_schema(),
            rows: Arc::new(Mutex::new(rows)),
        }
    }
}

#[async_trait]
impl SchemaSource for MemorySource {
    async fn describe(&self, _table: &str) -> Result<TableSchema, AdminError> {
        Ok(self.schema.clone())
    }

    async fn rows(&self, _table: &str, query: ListQuery) -> Result<ListResult, AdminError> {
        let db = self.rows.lock().unwrap();
        let page = query.page.unwrap_or(1).max(1);
        let per_page = query.per_page.unwrap_or(10).max(1);
        let mut all: Vec<Value> = db.iter().map(|r| json!(r)).collect();
        if let Some(term) = query.search.as_ref().filter(|s| !s.is_empty()) {
            let term = term.to_lowercase();
            all.retain(|row| {
                row["name"]
                    .as_str()
                    .map(|name| name.to_lowercase().contains(&term))
                    .unwrap_or(false)
            });
        }
        let total = all.len() as u64;
        let skip = ((page - 1) * per_page) as usize;
        Ok(ListResult {
            rows: all.into_iter().skip(skip).take(per_page as usize).collect(),
            total,
            page,
            per_page,
        })
    }

    async fn row(&self, _table: &str, id: &str) -> Result<Value, AdminError> {
        let db = self.rows.lock().unwrap();
        db.iter()
            .find(|row| row["id"] == json!(id))
            .map(|row| json!(row))
            .ok_or(AdminError::NotFound)
    }

    async fn insert(&self, _table: &str, data: RowMap) -> Result<Value, AdminError> {
        let mut db = self.rows.lock().unwrap();
        let id = (db.len() + 1).to_string();
        let mut row = data;
        row.insert("id".into(), json!(id));
        db.push(row);
        Ok(json!({ "id": id }))
    }

    async fn update(&self, _table: &str, id: &str, data: RowMap) -> Result<Value, AdminError> {
        let mut db = self.rows.lock().unwrap();
        let row = db
            .iter_mut()
            .find(|row| row["id"] == json!(id))
            .ok_or(AdminError::NotFound)?;
        for (key, value) in data {
            row.insert(key, value);
        }
        Ok(json!({ "id": id }))
    }

    async fn delete(&self, _table: &str, id: &str) -> Result<(), AdminError> {
        let mut db = self.rows.lock().unwrap();
        let before = db.len();
        db.retain(|row| row["id"] != json!(id));
        if db.len() == before {
            return Err(AdminError::NotFound);
        }
        Ok(())
    }

    async fn options(
        &self,
        _table: &str,
        _value_column: &str,
    ) -> Result<Vec<(String, String)>, AdminError> {
        Ok(vec![
            ("1".into(), "Food".into()),
            ("2".into(), "Tools".into()),
        ])
    }
}

async fn table() -> GenericTable {
    let source: Arc<dyn SchemaSource> = Arc::new(MemorySource::new());
    GenericTable::new(source, "products").await.unwrap()
}

#[tokio::test]
async fn list_columns_follow_the_schema() {
    let table = table().await;
    let columns = table.list_columns();
    assert_eq!(columns.len(), 5);
    assert_eq!(columns[0].key, "id");
    assert_eq!(columns[3].key, "active");
    assert_eq!(columns[4].key, "category_id");
}

#[tokio::test]
async fn form_fields_skip_primary_key_and_mark_required() {
    let table = table().await;
    let fields = table.form_fields();
    let keys: Vec<&str> = fields.iter().map(|f| f.key.as_str()).collect();
    assert_eq!(keys, vec!["name", "price", "active", "category_id"]);

    let required: Vec<&str> = fields
        .iter()
        .filter(|f| f.required)
        .map(|f| f.key.as_str())
        .collect();
    assert_eq!(required, vec!["name", "active"]);
}

#[tokio::test]
async fn foreign_keys_become_selects_with_database_options() {
    let table = table().await;
    let fields = table.form_fields_dynamic().await;
    let category = fields.iter().find(|f| f.key == "category_id").unwrap();
    match &category.field_type {
        FieldType::Select { options } => {
            assert_eq!(options.len(), 2);
            assert_eq!(options[0], ("1".to_string(), "Food".to_string()));
        }
        other => panic!("expected a Select, got {:?}", other),
    }
}

#[tokio::test]
async fn nullable_columns_are_optional() {
    let table = table().await;
    let fields = table.form_fields();
    let price = fields.iter().find(|f| f.key == "price").unwrap();
    assert!(
        !price.required,
        "price is nullable and must not be required"
    );
}

#[tokio::test]
async fn create_normalizes_booleans_and_empty_values() {
    let table = table().await;
    let mut data = HashMap::new();
    data.insert("name".to_string(), json!("Coffee"));
    data.insert("price".to_string(), json!(""));
    data.insert("active".to_string(), json!("on"));
    data.insert("category_id".to_string(), json!("2"));

    let created = table.create(data).await.unwrap();
    let id = created["id"].as_str().unwrap().to_string();

    let row = table.get(&id).await.unwrap();
    assert_eq!(row["name"], json!("Coffee"));
    assert_eq!(row["active"], json!(true));
    assert_eq!(row["price"], Value::Null);
    assert_eq!(row["category_id"], json!("2"));
}

#[tokio::test]
async fn validate_reports_missing_required_fields() {
    let table = table().await;
    let mut data = HashMap::new();
    data.insert("name".to_string(), json!(""));
    data.insert("active".to_string(), json!("on"));

    let errors = table.validate(&data).await.unwrap_err();
    assert!(errors.contains_key("name"));
    assert!(!errors.contains_key("active"));
}

#[tokio::test]
async fn list_runs_the_query_through_the_source() {
    let table = table().await;
    let result = table
        .list(ListQuery {
            search: Some("tea".into()),
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(result.total, 1);
    assert_eq!(result.rows[0]["name"], json!("Tea"));
}

#[tokio::test]
async fn delete_missing_row_is_not_found() {
    let table = table().await;
    assert!(matches!(
        table.delete("999").await,
        Err(AdminError::NotFound)
    ));
}

#[tokio::test]
async fn info_exposes_can_create_and_can_delete_for_the_templates() {
    let table = table().await;
    let info = serde_json::to_value(table.info("admin")).unwrap();
    assert_eq!(info["can_create"], json!(true));
    assert_eq!(info["can_delete"], json!(true));
    assert_eq!(info["path_list"], json!("/admin/products/"));
}

#[test]
fn form_fields_serialize_flat_for_tera() {
    let select = serde_json::to_value(FormField::select(
        "category",
        "Category",
        vec![("1", "Food")],
    ))
    .unwrap();
    assert_eq!(select["field_type"], json!("Select"));
    assert_eq!(select["options"][0][1], json!("Food"));

    let textarea = serde_json::to_value(FormField::textarea("body", "Body", 4)).unwrap();
    assert_eq!(textarea["field_type"], json!("TextArea"));
    assert_eq!(textarea["rows"], json!(4));

    let date = serde_json::to_value(FormField::date("when", "When")).unwrap();
    assert_eq!(date["field_type"], json!("Date"));
}

/// Regression test: the templates compare `field.field_type` against a string,
/// so Select/TextArea used to fall through to the plain input branch.
#[test]
fn tera_branches_on_field_type() {
    let mut tera = tera::Tera::default();
    tera.add_raw_template(
        "t",
        "{% if f.field_type == 'Select' %}{% for o in f.options %}{{ o.0 }}={{ o.1 }};{% endfor %}\
         {% elif f.field_type == 'TextArea' %}rows={{ f.rows }}{% endif %}",
    )
    .unwrap();

    let mut ctx = tera::Context::new();
    ctx.insert("f", &FormField::select("c", "C", vec![("1", "Food")]));
    assert_eq!(tera.render("t", &ctx).unwrap(), "1=Food;");

    let mut ctx = tera::Context::new();
    ctx.insert("f", &FormField::textarea("body", "Body", 4));
    assert_eq!(tera.render("t", &ctx).unwrap(), "rows=4");
}
