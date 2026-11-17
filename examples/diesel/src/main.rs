use std::sync::Arc;
use actix_web::{web, App, HttpServer};
use actix_session::{SessionMiddleware, storage::CookieSessionStore};
use actix_crud_admin::{AdminSite, init_templates};
use actix_crud_admin::auth::{JsonUserStore, UserStore};
use diesel_migrations::{embed_migrations, EmbeddedMigrations, MigrationHarness};

mod admin;
mod db;
mod models;
mod schema;

pub const MIGRATIONS: EmbeddedMigrations = embed_migrations!("migrations");

fn secret_key() -> actix_web::cookie::Key {
    let key_str = std::env::var("ADMIN_SECRET_KEY")
        .unwrap_or_else(|_| "a_very_long_secret_key_that_is_exactly_64_bytes_long_123456789012".to_string());
    let key_bytes = key_str.as_bytes();
    actix_web::cookie::Key::from(key_bytes)
}

async fn ensure_admin_user(store: &JsonUserStore) {
    if store.find_by_username("admin").await.unwrap().is_none() {
        store.create_user("admin", "admin@example.com", "Admin", "admin", true).await.unwrap();
    }
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    dotenvy::dotenv().ok();
    env_logger::init();

    let pool = Arc::new(db::create_pool());

    // Run Diesel migrations on startup.
    {
        let mut conn = pool.get().expect("Failed to get DB connection for migrations");
        conn.run_pending_migrations(MIGRATIONS).expect("Failed to run migrations");
    }

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
                    .title("Diesel Example Admin")
                    .with_user_store(store.clone())
                    .mount(cfg, registry.clone())
            })
    })
    .bind(("127.0.0.1", 8080))?
    .run()
    .await
}
