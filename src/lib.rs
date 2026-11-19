//! # actix-crud-admin
//!
//! A powerful admin panel library for Actix-web 4 with a pluggable authentication system.
//!
//! ## Quick Start
//!
//! ```rust,no_run
//! use actix_web::{web, App, HttpServer};
//! use actix_crud_admin::auth::{UserStore, JsonUserStore};
//! use actix_crud_admin::{AdminRegistry, AdminSite, init_templates};
//! use std::sync::Arc;
//!
//! #[actix_web::main]
//! async fn main() -> std::io::Result<()> {
//!     let templates = init_templates();
//!
//!     let store = Arc::new(JsonUserStore::new("users.json"));
//!     if store.find_by_username("admin").await.unwrap().is_none() {
//!         store.create_user("admin", "admin@example.com", "Admin", "admin", true).await.unwrap();
//!     }
//!
//!     HttpServer::new(move || {
//!         let mut registry = AdminRegistry::new();
//!         // Register your resources...
//!
//!         App::new()
//!             .app_data(web::Data::new(templates.clone()))
//!             .app_data(web::Data::new(store.clone() as Arc<dyn UserStore>))
//!             .wrap(actix_crud_admin::auth::session_middleware(
//!                 actix_web::cookie::Key::generate(),
//!             ))
//!             .configure(|cfg| {
//!                 AdminSite::new("/admin")
//!                     .with_user_store(store.clone())
//!                     .mount(cfg, registry)
//!             })
//!     })
//!     .bind(("127.0.0.1", 8080))?
//!     .run()
//!     .await
//! }
//! ```

//! ## Guías
//!
//! - [De la tabla al panel](https://github.com/TincoNomad/actix-crud-admin/blob/main/docs/introspection.md)
//!   — admin automático desde una tabla existente.
//! - [Formularios y base de datos](https://github.com/TincoNomad/actix-crud-admin/blob/main/docs/forms.md)
//!   — recursos escritos a mano.
//! - [Quick start](https://github.com/TincoNomad/actix-crud-admin/blob/main/docs/quick-start.md)

pub mod audit;
pub mod auth;
pub mod cli;
pub mod generic;
pub mod handlers;
pub mod labels;
pub mod registry;
pub mod resource;
pub mod schema;
pub mod site;
#[cfg(feature = "sqlite")]
pub mod sqlite;
#[cfg(feature = "postgres")]
pub mod postgres;
pub mod theme;
pub mod types;

pub use audit::{AuditEntry, AuditStore, JsonAuditLog};
pub use generic::GenericTable;
pub use labels::Labels;
pub use registry::AdminRegistry;
pub use resource::AdminResource;
pub use schema::SchemaSource;
pub use site::AdminSite;
pub use theme::Theme;

use std::sync::Arc;
use tera::Tera;

/// Helper to create a Tera instance with embedded templates.
pub fn init_templates() -> Tera {
    let mut tera = Tera::default();

    tera.add_raw_template("base.html", include_str!("templates/base.html"))
        .unwrap();
    tera.add_raw_template("login.html", include_str!("templates/login.html"))
        .unwrap();
    tera.add_raw_template("dashboard.html", include_str!("templates/dashboard.html"))
        .unwrap();
    tera.add_raw_template("list.html", include_str!("templates/list.html"))
        .unwrap();
    tera.add_raw_template("form.html", include_str!("templates/form.html"))
        .unwrap();
    tera.add_raw_template("password.html", include_str!("templates/password.html"))
        .unwrap();
    tera.add_raw_template("audit.html", include_str!("templates/audit.html"))
        .unwrap();

    tera
}

pub type AdminTemplates = Arc<Tera>;
