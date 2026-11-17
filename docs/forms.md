# Forms and Database Integration

This guide explains how to build create/edit forms and connect them to your own database backend, following a modular architecture inspired by Django.

## Recommended project structure

When you add `actix-crud-admin` to an existing project, keep admin-related code in a dedicated module. This separates the admin layer from your domain models and database code.

```text
your-project/
├── Cargo.toml
├── src/
│   ├── main.rs              # HttpServer, AdminSite wiring
│   ├── models/              # Database models (Diesel, SQLx, etc.)
│   │   ├── mod.rs
│   │   └── product.rs
│   ├── admin/               # AdminResource implementations
│   │   ├── mod.rs           # Registry setup
│   │   └── product.rs       # ProductAdmin resource
│   └── db.rs                # Connection pool setup
└── migrations/              # Database migrations
```

This mirrors the Django pattern where `models.py` and `admin.py` live next to each other.

## Form fields are manual

Like Django's `ModelAdmin` fields, form fields are declared explicitly. The crate does not auto-generate them from a database schema. You define them in `AdminResource::form_fields()`:

```rust
use actix_crud_admin::types::FormField;

fn form_fields(&self) -> Vec<FormField> {
    vec![
        FormField::text("name", "Name")
            .required()
            .help("The product name as shown to customers."),
        FormField::number("price", "Price")
            .required()
            .placeholder("0.00"),
        FormField::boolean("active", "Active")
            .help("Uncheck to hide the product from the storefront."),
        FormField::select("category", "Category", vec![
            ("electronics", "Electronics"),
            ("clothing", "Clothing"),
            ("food", "Food"),
        ]).required(),
    ]
}
```

Available field types:

| Builder | HTML input |
|---|---|
| `FormField::text` | `<input type="text">` |
| `FormField::number` | `<input type="number">` |
| `FormField::email` | `<input type="email">` |
| `FormField::password` | `<input type="password">` |
| `FormField::textarea(rows)` | `<textarea>` |
| `FormField::boolean` | `<input type="checkbox">` |
| `FormField::date` | `<input type="date">` |
| `FormField::select` | `<select>` |

Each builder supports `.required()`, `.placeholder("...")`, `.help("...")`, and `.fieldset("Group name")` for future grouping support.

## Validation

Add a `validate()` implementation to show per-field errors in the Django-style form:

```rust
use std::collections::HashMap;
use actix_crud_admin::types::{AdminError, Column, FormField, ListQuery, ListResult};

async fn validate(
    &self,
    data: &HashMap<String, serde_json::Value>,
) -> Result<(), HashMap<String, String>> {
    let mut errors = HashMap::new();

    if data.get("name").and_then(|v| v.as_str()).map(|s| s.trim().is_empty()).unwrap_or(true) {
        errors.insert("name".to_string(), "Name is required.".to_string());
    }

    if data.get("price").and_then(|v| v.as_str()).and_then(|s| s.parse::<f64>().ok()).unwrap_or(0.0) <= 0.0 {
        errors.insert("price".to_string(), "Price must be greater than zero.".to_string());
    }

    if errors.is_empty() { Ok(()) } else { Err(errors) }
}
```

## Connecting to Diesel/PostgreSQL

The crate does not include a database driver. You use your existing database stack and implement `AdminResource` over it.

### 1. Model and schema

```rust
// src/models/product.rs
use diesel::prelude::*;
use serde::Serialize;

#[derive(Queryable, Selectable, Serialize, Insertable, AsChangeset, Identifiable)]
#[diesel(table_name = crate::schema::products)]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct Product {
    pub id: i32,
    pub name: String,
    pub price: f64,
    pub active: bool,
    pub category: String,
}

#[derive(Insertable)]
#[diesel(table_name = crate::schema::products)]
pub struct NewProduct {
    pub name: String,
    pub price: f64,
    pub active: bool,
    pub category: String,
}
```

### 2. Admin resource

```rust
// src/admin/product.rs
use std::collections::HashMap;
use std::sync::Arc;
use async_trait::async_trait;
use diesel::prelude::*;
use actix_crud_admin::types::{AdminError, Column, FormField, ListQuery, ListResult};
use actix_crud_admin::resource::AdminResource;
use crate::models::product::{Product, NewProduct};
use crate::schema::products;

pub struct ProductAdmin {
    pool: Arc<diesel::r2d2::Pool<diesel::r2d2::ConnectionManager<PgConnection>>>,
}

impl ProductAdmin {
    pub fn new(pool: Arc<diesel::r2d2::Pool<diesel::r2d2::ConnectionManager<PgConnection>>>) -> Self {
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
            FormField::text("name", "Name").required(),
            FormField::number("price", "Price").required(),
            FormField::boolean("active", "Active"),
            FormField::select("category", "Category", vec![
                ("electronics", "Electronics"),
                ("clothing", "Clothing"),
                ("food", "Food"),
            ]).required(),
        ]
    }

    async fn list(&self, query: ListQuery) -> Result<ListResult, AdminError> {
        use crate::schema::products::dsl::*;
        let mut conn = self.pool.get().map_err(|e| AdminError::DatabaseError(e.to_string()))?;

        let mut rows: Vec<serde_json::Value> = products
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
                r["name"].as_str().map(|n| n.to_lowercase().contains(&s)).unwrap_or(false)
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

    async fn get(&self, id: &str) -> Result<serde_json::Value, AdminError> {
        let id = id.parse::<i32>().map_err(|_| AdminError::NotFound)?;
        let mut conn = self.pool.get().map_err(|e| AdminError::DatabaseError(e.to_string()))?;
        let p: Product = crate::schema::products::dsl::products
            .find(id)
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
        let p: Product = diesel::insert_into(products::table)
            .values(&new)
            .get_result(&mut conn)
            .map_err(|e| AdminError::DatabaseError(e.to_string()))?;
        Ok(serde_json::json!({ "id": p.id }))
    }

    async fn update(&self, id: &str, data: HashMap<String, serde_json::Value>) -> Result<serde_json::Value, AdminError> {
        let id = id.parse::<i32>().map_err(|_| AdminError::NotFound)?;
        let mut conn = self.pool.get().map_err(|e| AdminError::DatabaseError(e.to_string()))?;
        let p: Product = diesel::update(products::table.find(id))
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

    async fn delete(&self, id: &str) -> Result<(), AdminError> {
        let id = id.parse::<i32>().map_err(|_| AdminError::NotFound)?;
        let mut conn = self.pool.get().map_err(|e| AdminError::DatabaseError(e.to_string()))?;
        diesel::delete(products::table.find(id))
            .execute(&mut conn)
            .map_err(|e| AdminError::DatabaseError(e.to_string()))?;
        Ok(())
    }
}
```

### 3. Registry module

```rust
// src/admin/mod.rs
use std::sync::Arc;
use actix_crud_admin::registry::AdminRegistry;
use diesel::r2d2::{ConnectionManager, Pool};
use diesel::PgConnection;
use crate::admin::product::ProductAdmin;

pub mod product;

pub fn build_registry(pool: Arc<Pool<ConnectionManager<PgConnection>>>) -> AdminRegistry {
    let mut registry = AdminRegistry::new();
    registry.register(ProductAdmin::new(pool));
    registry
}
```

### 4. Wire it in main.rs

```rust
// src/main.rs
use std::sync::Arc;
use actix_web::{web, App, HttpServer};
use actix_session::{SessionMiddleware, storage::CookieSessionStore};
use actix_crud_admin::{AdminSite, init_templates};
use actix_crud_admin::auth::{JsonUserStore, UserStore};
use diesel::r2d2::{ConnectionManager, Pool};
use diesel::PgConnection;

mod admin;
mod models;
mod schema;
mod db;

async fn ensure_admin_user(store: &JsonUserStore) {
    if store.find_by_username("admin").await.unwrap().is_none() {
        store.create_user("admin", "admin@example.com", "Admin", "admin", true).await.unwrap();
    }
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    env_logger::init();

    let pool = Arc::new(db::create_pool());
    let templates = init_templates();
    let registry = admin::build_registry(pool.clone());

    let store = Arc::new(JsonUserStore::new("users.json"));
    ensure_admin_user(&store).await;

    HttpServer::new(move || {
        App::new()
            .app_data(web::Data::new(templates.clone()))
            .wrap(SessionMiddleware::new(CookieSessionStore::default(), secret_key()))
            .configure(|cfg| {
                AdminSite::new("/admin")
                    .title("My Admin")
                    .with_user_store(store.clone())
                    .mount(cfg, registry.clone())
            })
    })
    .bind(("127.0.0.1", 8080))?
    .run()
    .await
}
```

## Form rendering

The crate renders forms using `src/templates/form.html`. The current template follows a Django-inspired layout:

- Breadcrumbs at the top.
- One form field per row with the label above the input.
- Help text and validation errors below each input.
- A submit row with **Save**, **Save and continue editing**, **Save and add another**, and **Cancel**.
- A separate **Delete** button when editing.

You can override the template by providing your own `Tera` instance with a template named `form.html`.

## Next steps

- See [examples/diesel](examples/diesel) for a runnable Diesel/PostgreSQL example.
- See [Advanced Configuration](advanced.md) for custom `UserStore` backends and authentication.
