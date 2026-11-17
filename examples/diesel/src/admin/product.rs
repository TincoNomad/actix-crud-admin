use std::collections::HashMap;
use std::sync::Arc;
use async_trait::async_trait;
use diesel::prelude::*;
use actix_crud_admin::resource::AdminResource;
use actix_crud_admin::types::{AdminError, Column, FormField, ListQuery, ListResult};
use crate::db::DbPool;
use crate::models::product::{Product, NewProduct};
use crate::schema::products::dsl::*;

pub struct ProductAdmin {
    pool: Arc<DbPool>,
}

impl ProductAdmin {
    pub fn new(pool: Arc<DbPool>) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl AdminResource for ProductAdmin {
    fn name(&self) -> &str { "Product" }
    fn plural_name(&self) -> &str { "Products" }
    fn slug(&self) -> &str { "products" }
    fn icon(&self) -> &str { "📦" }

    fn list_columns(&self) -> Vec<Column> {
        vec![
            Column::text("name", "Name"),
            Column::number("price", "Price"),
            Column::boolean("active", "Active"),
            Column::text("category", "Category"),
        ]
    }

    fn form_fields(&self) -> Vec<FormField> {
        vec![
            FormField::text("name", "Name")
                .required()
                .help("Product name shown to customers."),
            FormField::number("price", "Price")
                .required()
                .placeholder("0.00"),
            FormField::boolean("active", "Active")
                .help("Visible in the storefront when checked."),
            FormField::select("category", "Category", vec![
                ("electronics", "Electronics"),
                ("clothing", "Clothing"),
                ("food", "Food"),
            ]).required(),
        ]
    }

    async fn validate(
        &self,
        data: &HashMap<String, serde_json::Value>,
    ) -> Result<(), HashMap<String, String>> {
        let mut errors = HashMap::new();

        if data.get("name").and_then(|v| v.as_str()).map(|s| s.trim().is_empty()).unwrap_or(true) {
            errors.insert("name".to_string(), "Name is required.".to_string());
        }

        let price_value = data
            .get("price")
            .and_then(|v| v.as_str())
            .and_then(|s| s.parse::<f64>().ok())
            .unwrap_or(0.0);
        if price_value <= 0.0 {
            errors.insert("price".to_string(), "Price must be greater than zero.".to_string());
        }

        if errors.is_empty() { Ok(()) } else { Err(errors) }
    }

    async fn list(&self, query: ListQuery) -> Result<ListResult, AdminError> {
        let mut conn = self.pool.get().map_err(|e| AdminError::DatabaseError(e.to_string()))?;

        let mut rows: Vec<serde_json::Value> = products
            .order_by(crate::schema::products::created_at.desc())
            .load::<Product>(&mut conn)
            .map_err(|e| AdminError::DatabaseError(e.to_string()))?
            .into_iter()
            .map(|p| serde_json::json!({
                "id": p.id,
                "name": p.name,
                "price": p.price,
                "active": p.active,
                "category": p.category,
            }))
            .collect();

        if let Some(ref s) = query.search {
            let s = s.to_lowercase();
            rows.retain(|r| {
                r["name"].as_str().map(|n: &str| n.to_lowercase().contains(&s)).unwrap_or(false)
                    || r["category"].as_str().map(|c: &str| c.to_lowercase().contains(&s)).unwrap_or(false)
            });
        }

        let total = rows.len() as u64;
        let per_page = query.per_page.unwrap_or(10);
        let page = query.page.unwrap_or(1);
        let start = ((page - 1) * per_page) as usize;
        let rows = if start < rows.len() {
            let end = (start + per_page as usize).min(rows.len());
            rows[start..end].to_vec()
        } else {
            vec![]
        };

        Ok(ListResult { rows, total, page, per_page })
    }

    async fn get(&self, record_id: &str) -> Result<serde_json::Value, AdminError> {
        let record_id = record_id.parse::<i32>().map_err(|_| AdminError::NotFound)?;
        let mut conn = self.pool.get().map_err(|e| AdminError::DatabaseError(e.to_string()))?;
        let p: Product = products
            .find(record_id)
            .first(&mut conn)
            .map_err(|e| match e {
                diesel::result::Error::NotFound => AdminError::NotFound,
                _ => AdminError::DatabaseError(e.to_string()),
            })?;
        Ok(serde_json::json!({
            "id": p.id,
            "name": p.name,
            "price": p.price,
            "active": p.active,
            "category": p.category,
        }))
    }

    async fn create(&self, data: HashMap<String, serde_json::Value>) -> Result<serde_json::Value, AdminError> {
        let mut conn = self.pool.get().map_err(|e| AdminError::DatabaseError(e.to_string()))?;
        let new = NewProduct {
            name: data.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            price: data.get("price").and_then(|v| v.as_str()).and_then(|s| s.parse().ok()).unwrap_or(0.0),
            active: data.get("active").and_then(|v| v.as_bool()).unwrap_or(false),
            category: data.get("category").and_then(|v| v.as_str()).unwrap_or("").to_string(),
        };
        let p: Product = diesel::insert_into(crate::schema::products::table)
            .values(&new)
            .get_result(&mut conn)
            .map_err(|e| AdminError::DatabaseError(e.to_string()))?;
        Ok(serde_json::json!({ "id": p.id }))
    }

    async fn update(&self, record_id: &str, data: HashMap<String, serde_json::Value>) -> Result<serde_json::Value, AdminError> {
        let record_id = record_id.parse::<i32>().map_err(|_| AdminError::NotFound)?;
        let mut conn = self.pool.get().map_err(|e| AdminError::DatabaseError(e.to_string()))?;
        let p: Product = diesel::update(products.find(record_id))
            .set(&NewProduct {
                name: data.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                price: data.get("price").and_then(|v| v.as_str()).and_then(|s| s.parse().ok()).unwrap_or(0.0),
                active: data.get("active").and_then(|v| v.as_bool()).unwrap_or(false),
                category: data.get("category").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            })
            .get_result(&mut conn)
            .map_err(|e| match e {
                diesel::result::Error::NotFound => AdminError::NotFound,
                _ => AdminError::DatabaseError(e.to_string()),
            })?;
        Ok(serde_json::json!({ "id": p.id }))
    }

    async fn delete(&self, record_id: &str) -> Result<(), AdminError> {
        let record_id = record_id.parse::<i32>().map_err(|_| AdminError::NotFound)?;
        let mut conn = self.pool.get().map_err(|e| AdminError::DatabaseError(e.to_string()))?;
        diesel::delete(products.find(record_id))
            .execute(&mut conn)
            .map_err(|e| AdminError::DatabaseError(e.to_string()))?;
        Ok(())
    }
}
