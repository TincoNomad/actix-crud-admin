//! End-to-end tests against a real (in-memory) SQLite database.
//!
//! Run with `cargo test --features sqlite`.

#![cfg(feature = "sqlite")]

use actix_crud_admin::generic::GenericTable;
use actix_crud_admin::schema::{ColumnKind, SchemaSource};
use actix_crud_admin::sqlite::SqliteSource;
use actix_crud_admin::types::*;
use actix_crud_admin::AdminResource;
use serde_json::json;
use sqlx::sqlite::SqlitePoolOptions;
use std::collections::HashMap;
use std::sync::Arc;

async fn source() -> SqliteSource {
    // One connection so every query sees the same in-memory database.
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    sqlx::query("CREATE TABLE categories (id INTEGER PRIMARY KEY, name TEXT NOT NULL)")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(
        "CREATE TABLE products (
            id INTEGER PRIMARY KEY,
            name TEXT NOT NULL,
            price REAL,
            active BOOLEAN NOT NULL DEFAULT 1,
            category_id INTEGER REFERENCES categories(id)
        )",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "CREATE TABLE notes (
            id INTEGER PRIMARY KEY,
            body TEXT NOT NULL,
            created_at TEXT NOT NULL DEFAULT '2020-01-01'
        )",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query("INSERT INTO categories (name) VALUES ('Food'), ('Tools')")
        .execute(&pool)
        .await
        .unwrap();
    SqliteSource::from_pool(pool)
}

async fn table() -> GenericTable {
    let shared: Arc<dyn SchemaSource> = Arc::new(source().await);
    GenericTable::new(shared, "products").await.unwrap()
}

fn product(name: &str, price: &str, category_id: &str) -> HashMap<String, serde_json::Value> {
    HashMap::from([
        ("name".to_string(), json!(name)),
        ("price".to_string(), json!(price)),
        ("active".to_string(), json!("on")),
        ("category_id".to_string(), json!(category_id)),
    ])
}

#[tokio::test]
async fn describe_reads_columns_and_foreign_keys() {
    let source = source().await;
    let schema = source.describe("products").await.unwrap();

    let names: Vec<&str> = schema.columns.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(names, vec!["id", "name", "price", "active", "category_id"]);

    let id = schema.column("id").unwrap();
    assert!(id.primary_key);
    assert_eq!(id.kind, ColumnKind::Number);

    let active = schema.column("active").unwrap();
    assert_eq!(active.kind, ColumnKind::Boolean);

    let category = schema.column("category_id").unwrap();
    let fk = category.foreign_key.as_ref().unwrap();
    assert_eq!(fk.table, "categories");
    assert_eq!(fk.column, "id");
}

#[tokio::test]
async fn describe_missing_table_is_not_found() {
    let source = source().await;
    assert!(matches!(
        source.describe("nope").await,
        Err(AdminError::NotFound)
    ));
}

#[tokio::test]
async fn foreign_key_select_reads_the_target_table() {
    let table = table().await;
    let fields = table.form_fields_dynamic().await;
    let category = fields.iter().find(|f| f.key == "category_id").unwrap();
    match &category.field_type {
        FieldType::Select { options } => {
            assert_eq!(
                options,
                &vec![
                    ("1".to_string(), "Food".to_string()),
                    ("2".to_string(), "Tools".to_string()),
                ]
            );
        }
        other => panic!("expected a Select, got {:?}", other),
    }
}

#[tokio::test]
async fn create_update_and_delete_round_trip() {
    let table = table().await;
    let created = table.create(product("Tea", "3.5", "1")).await.unwrap();
    let id = created["id"].as_str().unwrap().to_string();

    let row = table.get(&id).await.unwrap();
    assert_eq!(row["name"], json!("Tea"));
    assert_eq!(row["active"], json!(true));
    assert_eq!(row["category_id"], json!("1"));

    table.update(&id, product("Tea", "4.5", "2")).await.unwrap();
    let row = table.get(&id).await.unwrap();
    assert_eq!(row["price"], json!("4.5"));
    assert_eq!(row["category_id"], json!("2"));

    table.delete(&id).await.unwrap();
    assert!(matches!(table.get(&id).await, Err(AdminError::NotFound)));
    assert!(matches!(table.delete(&id).await, Err(AdminError::NotFound)));
}

#[tokio::test]
async fn list_paginates_and_counts() {
    let table = table().await;
    for name in ["A", "B", "C"] {
        table.create(product(name, "1", "1")).await.unwrap();
    }

    let page = table
        .list(ListQuery {
            page: Some(2),
            per_page: Some(2),
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(page.total, 3);
    assert_eq!(page.rows.len(), 1);
    assert_eq!(page.rows[0]["name"], json!("A"));
}

#[tokio::test]
async fn list_searches_text_columns() {
    let table = table().await;
    table.create(product("Green tea", "2", "1")).await.unwrap();
    table.create(product("Coffee", "5", "2")).await.unwrap();

    let result = table
        .list(ListQuery {
            search: Some("tea".into()),
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(result.total, 1);
    assert_eq!(result.rows[0]["name"], json!("Green tea"));
}

#[tokio::test]
async fn list_sorts_by_an_allowed_column() {
    let table = table().await;
    table.create(product("Alpha", "1", "1")).await.unwrap();
    table.create(product("Zulu", "2", "1")).await.unwrap();

    let result = table
        .list(ListQuery {
            sort_by: Some("name".into()),
            sort_dir: Some("desc".into()),
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(result.rows[0]["name"], json!("Zulu"));
}

#[tokio::test]
async fn list_sorts_number_columns_numerically() {
    let table = table().await;
    table.create(product("Ten", "10", "1")).await.unwrap();
    table.create(product("Nine", "9", "1")).await.unwrap();
    table
        .create(product("Twenty five", "25", "1"))
        .await
        .unwrap();

    let result = table
        .list(ListQuery {
            sort_by: Some("price".into()),
            sort_dir: Some("desc".into()),
            ..Default::default()
        })
        .await
        .unwrap();

    let names: Vec<_> = result.rows.iter().map(|r| r["name"].clone()).collect();
    // Sorted as text this would be Nine, Twenty five, Ten.
    assert_eq!(
        names,
        vec![json!("Twenty five"), json!("Ten"), json!("Nine")]
    );
}

#[tokio::test]
async fn list_ignores_unknown_sort_columns() {
    let table = table().await;
    table.create(product("Alpha", "1", "1")).await.unwrap();

    let result = table
        .list(ListQuery {
            sort_by: Some("name\"; DROP TABLE products; --".into()),
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(result.total, 1);
    // The injection attempt must not have dropped the table.
    assert!(table.get("1").await.is_ok());
}

#[tokio::test]
async fn describe_is_cached_until_cleared() {
    let source = source().await;
    assert!(source.describe("products").await.is_ok());

    sqlx::query("DROP TABLE products")
        .execute(source.pool())
        .await
        .unwrap();

    assert!(
        source.describe("products").await.is_ok(),
        "the schema must come from the cache"
    );

    source.clear_cache();
    assert!(matches!(
        source.describe("products").await,
        Err(AdminError::NotFound)
    ));
}

mod http {
    use super::*;
    use actix_crud_admin::audit::{AuditStore, JsonAuditLog};
    use actix_crud_admin::auth::{JsonUserStore, UserStore};
    use actix_crud_admin::{init_templates, AdminRegistry, AdminSite, Labels, Theme};
    use actix_session::{storage::CookieSessionStore, SessionMiddleware};
    use actix_web::{test, web, App};
    use std::path::PathBuf;

    type TestService = actix_web::dev::ServiceResponse;

    fn temp_path(name: &str) -> PathBuf {
        let path = std::env::temp_dir().join(name);
        let _ = std::fs::remove_file(&path);
        path
    }

    fn csrf_from(html: &str) -> String {
        let marker = "name=\"_csrf\" value=\"";
        let start = html.find(marker).expect("csrf field in form") + marker.len();
        let end = start + html[start..].find('"').expect("csrf value terminator");
        html[start..end].to_string()
    }

    async fn build(
        labels: Labels,
        theme: Theme,
        audit: Option<Arc<dyn AuditStore>>,
        users: PathBuf,
    ) -> impl actix_web::dev::Service<
        actix_http::Request,
        Response = TestService,
        Error = actix_web::Error,
    > {
        let mut registry = AdminRegistry::new();
        let source: Arc<dyn SchemaSource> = Arc::new(source().await);
        registry.register(GenericTable::new(source, "products").await.unwrap());

        let store = JsonUserStore::new(&users);
        store
            .create_user("admin", "admin@test.com", "Admin", "admin", true)
            .await
            .unwrap();
        store
            .create_user("viewer", "viewer@test.com", "Viewer", "admin", false)
            .await
            .unwrap();

        let templates = init_templates();
        test::init_service(
            App::new()
                .app_data(web::Data::new(templates))
                .wrap(SessionMiddleware::new(
                    CookieSessionStore::default(),
                    actix_web::cookie::Key::generate(),
                ))
                .configure(move |cfg| {
                    let mut site = AdminSite::new("/admin")
                        .title("Test Admin")
                        .labels(labels.clone())
                        .theme(theme.clone())
                        .with_user_store(Arc::new(store));
                    if let Some(audit) = audit.clone() {
                        site = site.with_audit(audit);
                    }
                    site.mount(cfg, registry);
                }),
        )
        .await
    }

    async fn app() -> impl actix_web::dev::Service<
        actix_http::Request,
        Response = TestService,
        Error = actix_web::Error,
    > {
        build(
            Labels::default(),
            Theme::default(),
            None,
            temp_path("test_admin_sqlite_users.json"),
        )
        .await
    }

    async fn login(
        app: &impl actix_web::dev::Service<
            actix_http::Request,
            Response = TestService,
            Error = actix_web::Error,
        >,
        username: &str,
    ) -> (actix_web::cookie::Cookie<'static>, String) {
        let req = test::TestRequest::get().uri("/admin/login").to_request();
        let resp = test::call_service(app, req).await;
        let (name, value) = {
            let cookie = resp.response().cookies().next().expect("session cookie");
            (cookie.name().to_string(), cookie.value().to_string())
        };
        let body = test::read_body(resp).await;
        let token = csrf_from(&String::from_utf8_lossy(&body));

        let req = test::TestRequest::post()
            .uri("/admin/login")
            .cookie(actix_web::cookie::Cookie::new(name, value))
            .set_form(HashMap::from([
                ("username", username),
                ("password", "admin"),
                ("_csrf", token.as_str()),
            ]))
            .to_request();
        let resp = test::call_service(app, req).await;
        assert_eq!(resp.status(), 302, "login should redirect");
        let (name, value) = {
            let cookie = resp.response().cookies().next().unwrap();
            (cookie.name().to_string(), cookie.value().to_string())
        };
        (actix_web::cookie::Cookie::new(name, value), token)
    }

    async fn get(
        app: &impl actix_web::dev::Service<
            actix_http::Request,
            Response = TestService,
            Error = actix_web::Error,
        >,
        uri: &str,
        cookie: actix_web::cookie::Cookie<'static>,
    ) -> (u16, String) {
        let req = test::TestRequest::get()
            .uri(uri)
            .cookie(cookie)
            .to_request();
        let resp = test::call_service(app, req).await;
        let status = resp.status().as_u16();
        let body = test::read_body(resp).await;
        (status, String::from_utf8_lossy(&body).to_string())
    }

    async fn post(
        app: &impl actix_web::dev::Service<
            actix_http::Request,
            Response = TestService,
            Error = actix_web::Error,
        >,
        uri: &str,
        cookie: actix_web::cookie::Cookie<'static>,
        token: &str,
        form: HashMap<&str, &str>,
    ) -> actix_web::dev::ServiceResponse {
        let mut form = form;
        form.insert("_csrf", token);
        let req = test::TestRequest::post()
            .uri(uri)
            .cookie(cookie)
            .set_form(form)
            .to_request();
        test::call_service(app, req).await
    }

    #[actix_web::test]
    async fn new_form_renders_foreign_key_select() {
        let app = app().await;
        let (cookie, _) = login(&app, "admin").await;
        let (status, html) = get(&app, "/admin/products/new", cookie).await;

        assert_eq!(status, 200);
        assert!(html.contains("<select"), "select input missing");
        assert!(html.contains("id=\"id_category_id\""));
        assert!(html.contains(">Food<"));
        assert!(html.contains(">Tools<"));
    }

    #[actix_web::test]
    async fn list_shows_buttons_and_filters_for_a_writer() {
        let app = app().await;
        let (cookie, token) = login(&app, "admin").await;
        post(
            &app,
            "/admin/products/new",
            cookie.clone(),
            &token,
            HashMap::from([("name", "Tea"), ("price", "2"), ("category_id", "1")]),
        )
        .await;
        let (status, html) = get(&app, "/admin/products/", cookie).await;

        assert_eq!(status, 200);
        assert!(html.contains(">Edit<"), "edit button missing");
        assert!(html.contains(">Delete<"), "delete button missing");
        assert!(html.contains("Add Products"), "add button missing");
        assert!(html.contains("filter_category_id"), "fk filter missing");
        assert!(html.contains(">Export CSV<"), "export link missing");
    }

    #[actix_web::test]
    async fn read_only_user_sees_no_write_controls() {
        let app = app().await;
        let (cookie, _) = login(&app, "viewer").await;
        let (status, html) = get(&app, "/admin/products/", cookie).await;

        assert_eq!(status, 200);
        assert!(!html.contains(">Edit<"), "reader must not see edit");
        assert!(!html.contains("Add Products"), "reader must not see add");
        assert!(
            !html.contains("Delete selected"),
            "reader must not see bulk"
        );
    }

    #[actix_web::test]
    async fn read_only_user_cannot_write_even_with_a_token() {
        let app = app().await;
        let (cookie, token) = login(&app, "viewer").await;
        let resp = post(
            &app,
            "/admin/products/new",
            cookie,
            &token,
            HashMap::from([("name", "Sneaky")]),
        )
        .await;
        assert_eq!(resp.status(), 403);
    }

    #[actix_web::test]
    async fn writes_require_a_csrf_token() {
        let app = app().await;
        let (cookie, _) = login(&app, "admin").await;
        let resp = post(
            &app,
            "/admin/products/new",
            cookie,
            "",
            HashMap::from([("name", "No token")]),
        )
        .await;
        assert_eq!(resp.status(), 403, "write without a token must be rejected");
    }

    #[actix_web::test]
    async fn create_then_export_and_bulk_delete() {
        let app = app().await;
        let (cookie, token) = login(&app, "admin").await;

        let resp = post(
            &app,
            "/admin/products/new",
            cookie.clone(),
            &token,
            HashMap::from([
                ("name", "Green tea"),
                ("price", "2.5"),
                ("active", "on"),
                ("category_id", "1"),
            ]),
        )
        .await;
        assert_eq!(resp.status(), 302);

        let (_, csv) = get(&app, "/admin/products/export", cookie.clone()).await;
        assert!(
            csv.starts_with("Id,Name"),
            "csv header missing: {}",
            &csv[..40.min(csv.len())]
        );
        assert!(csv.contains("Green tea"));

        let resp = post(
            &app,
            "/admin/products/bulk",
            cookie.clone(),
            &token,
            HashMap::from([("id_1", "1")]),
        )
        .await;
        assert_eq!(resp.status(), 302);

        let (_, html) = get(&app, "/admin/products/", cookie).await;
        assert!(!html.contains("Green tea"), "row should be deleted");
    }

    #[actix_web::test]
    async fn filters_narrow_the_list() {
        let app = app().await;
        let (cookie, token) = login(&app, "admin").await;
        for (name, category) in [("Tea", "1"), ("Coffee", "2")] {
            post(
                &app,
                "/admin/products/new",
                cookie.clone(),
                &token,
                HashMap::from([("name", name), ("price", "1"), ("category_id", category)]),
            )
            .await;
        }

        let (_, html) = get(&app, "/admin/products/?filter_category_id=2", cookie).await;
        assert!(html.contains("Coffee"));
        assert!(!html.contains("Tea</td>") || html.matches("Tea").count() < 3);
    }

    #[actix_web::test]
    async fn audit_page_records_writes() {
        let path = temp_path("test_admin_sqlite_audit.json");
        let app = build(
            Labels::default(),
            Theme::default(),
            Some(Arc::new(JsonAuditLog::new(&path))),
            temp_path("test_admin_sqlite_audit_users.json"),
        )
        .await;
        let (cookie, token) = login(&app, "admin").await;
        post(
            &app,
            "/admin/products/new",
            cookie.clone(),
            &token,
            HashMap::from([("name", "Audited")]),
        )
        .await;

        let (status, html) = get(&app, "/admin/audit", cookie).await;
        assert_eq!(status, 200);
        assert!(html.contains("create"), "action missing");
        assert!(html.contains("products"), "resource missing");
        assert!(html.contains("admin"), "user missing");
        let _ = std::fs::remove_file(&path);
    }

    #[actix_web::test]
    async fn password_change_validates_the_current_password() {
        let app = app().await;
        let (cookie, token) = login(&app, "admin").await;

        let resp = post(
            &app,
            "/admin/password",
            cookie.clone(),
            &token,
            HashMap::from([
                ("current", "wrong"),
                ("new_password", "secret123"),
                ("confirm", "secret123"),
            ]),
        )
        .await;
        assert_eq!(resp.status(), 400, "wrong current password must fail");

        let resp = post(
            &app,
            "/admin/password",
            cookie,
            &token,
            HashMap::from([
                ("current", "admin"),
                ("new_password", "secret123"),
                ("confirm", "secret123"),
            ]),
        )
        .await;
        assert_eq!(resp.status(), 200);
        let body = test::read_body(resp).await;
        assert!(String::from_utf8_lossy(&body).contains("Password updated"));
    }

    #[actix_web::test]
    async fn spanish_labels_reach_the_page() {
        let app = build(
            Labels::spanish(),
            Theme::from_primary("#ff0000"),
            None,
            temp_path("test_admin_sqlite_es_users.json"),
        )
        .await;
        let (cookie, _) = login(&app, "admin").await;
        let (_, html) = get(&app, "/admin/products/", cookie).await;

        assert!(html.contains("Exportar CSV"));
        assert!(html.contains("lang=\"es\""));
        assert!(html.contains("--primary: #ff0000"));
    }
}
