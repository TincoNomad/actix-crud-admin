use actix_crud_admin::auth::{JsonUserStore, UserStore};
use actix_crud_admin::types::*;
use actix_crud_admin::{init_templates, AdminRegistry, AdminResource, AdminSite};
use actix_session::storage::CookieSessionStore;
use actix_session::SessionMiddleware;
use actix_web::{test, web, App};
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

struct MockResource {
    db: Arc<Mutex<Vec<serde_json::Value>>>,
}

#[async_trait]
impl AdminResource for MockResource {
    fn name(&self) -> &str {
        "Mock"
    }
    fn plural_name(&self) -> &str {
        "Mocks"
    }
    fn slug(&self) -> &str {
        "mock"
    }
    fn list_columns(&self) -> Vec<Column> {
        vec![Column::text("name", "Name")]
    }
    fn form_fields(&self) -> Vec<FormField> {
        vec![FormField::text("name", "Name")]
    }
    async fn list(&self, _: ListQuery) -> Result<ListResult, AdminError> {
        let db = self.db.lock().unwrap();
        Ok(ListResult {
            rows: db.clone(),
            total: db.len() as u64,
            page: 1,
            per_page: 10,
        })
    }
    async fn get(&self, id: &str) -> Result<serde_json::Value, AdminError> {
        let db = self.db.lock().unwrap();
        db.iter()
            .find(|r| r["id"].as_str().unwrap_or("") == id)
            .cloned()
            .ok_or(AdminError::NotFound)
    }
    async fn create(
        &self,
        data: HashMap<String, serde_json::Value>,
    ) -> Result<serde_json::Value, AdminError> {
        let mut db = self.db.lock().unwrap();
        let mut record = data;
        record.insert("id".to_string(), serde_json::Value::String("1".to_string()));
        let val = serde_json::to_value(&record).unwrap();
        db.push(val.clone());
        Ok(val)
    }
    async fn update(
        &self,
        id: &str,
        data: HashMap<String, serde_json::Value>,
    ) -> Result<serde_json::Value, AdminError> {
        let mut db = self.db.lock().unwrap();
        if let Some(r) = db.iter_mut().find(|r| r["id"].as_str().unwrap_or("") == id) {
            for (k, v) in data {
                r[k] = v;
            }
            return Ok(r.clone());
        }
        Err(AdminError::NotFound)
    }
    async fn delete(&self, id: &str) -> Result<(), AdminError> {
        let mut db = self.db.lock().unwrap();
        db.retain(|r| r["id"].as_str().unwrap_or("") != id);
        Ok(())
    }
}

async fn setup_app() -> impl actix_web::dev::Service<
    actix_http::Request,
    Response = actix_web::dev::ServiceResponse,
    Error = actix_web::Error,
> {
    let templates = init_templates();
    let mut registry = AdminRegistry::new();
    registry.register(MockResource {
        db: Arc::new(Mutex::new(vec![])),
    });

    let users_path = std::env::temp_dir().join("test_admin_handlers.json");
    let _ = std::fs::remove_file(&users_path);
    let store = JsonUserStore::new(&users_path);
    store
        .create_user("admin", "admin@test.com", "Admin", "admin", true)
        .await
        .unwrap();

    test::init_service(
        App::new()
            .app_data(web::Data::new(templates))
            .wrap(SessionMiddleware::new(
                CookieSessionStore::default(),
                actix_web::cookie::Key::generate(),
            ))
            .configure(|cfg| {
                AdminSite::new("/admin")
                    .title("Test Admin")
                    .with_user_store(Arc::new(store))
                    .mount(cfg, registry)
            }),
    )
    .await
}

#[actix_web::test]
async fn test_auth_flow() {
    let app = setup_app().await;

    let req = test::TestRequest::get().uri("/admin/login").to_request();
    let resp = test::call_service(&app, req).await;
    assert!(resp.status().is_success());
}

#[actix_web::test]
async fn test_protected_routes() {
    let app = setup_app().await;

    let req = test::TestRequest::get().uri("/admin/").to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 302, "should redirect to login");
}

fn csrf_from(html: &str) -> String {
    let marker = "name=\"_csrf\" value=\"";
    let start = html.find(marker).expect("csrf field in form") + marker.len();
    let end = start + html[start..].find('"').expect("csrf value terminator");
    html[start..end].to_string()
}

async fn page(
    app: &impl actix_web::dev::Service<
        actix_http::Request,
        Response = actix_web::dev::ServiceResponse,
        Error = actix_web::Error,
    >,
    uri: &str,
    cookie: actix_web::cookie::Cookie<'static>,
) -> String {
    let req = test::TestRequest::get()
        .uri(uri)
        .cookie(cookie)
        .to_request();
    let resp = test::call_service(app, req).await;
    let body = test::read_body(resp).await;
    String::from_utf8_lossy(&body).to_string()
}

/// Logs in and returns the session cookie plus the CSRF token.
async fn login_cookie(
    app: &impl actix_web::dev::Service<
        actix_http::Request,
        Response = actix_web::dev::ServiceResponse,
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
    let first = actix_web::cookie::Cookie::new(name, value);
    let body = test::read_body(resp).await;
    let token = csrf_from(&String::from_utf8_lossy(&body));

    let req = test::TestRequest::post()
        .uri("/admin/login")
        .cookie(first)
        .set_form(std::collections::HashMap::from([
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

#[actix_web::test]
async fn test_login_and_access_protected() {
    let app = setup_app().await;
    let (cookie, _) = login_cookie(&app, "admin").await;

    let req = test::TestRequest::get()
        .uri("/admin/")
        .cookie(cookie)
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert!(
        resp.status().is_success(),
        "authenticated user should access dashboard"
    );
}

#[actix_web::test]
async fn test_login_rejects_a_missing_csrf_token() {
    let app = setup_app().await;
    let req = test::TestRequest::post()
        .uri("/admin/login")
        .set_form(std::collections::HashMap::from([
            ("username", "admin"),
            ("password", "admin"),
        ]))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 403, "login without a token must be rejected");
}

#[actix_web::test]
async fn test_login_by_email() {
    let app = setup_app().await;
    let (cookie, _) = login_cookie(&app, "admin@test.com").await;
    let req = test::TestRequest::get()
        .uri("/admin/")
        .cookie(cookie)
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert!(
        resp.status().is_success(),
        "login by email should grant access"
    );
}

#[actix_web::test]
async fn test_new_form_renders() {
    let app = setup_app().await;
    let (cookie, _) = login_cookie(&app, "admin").await;
    let html = page(&app, "/admin/mock/new", cookie).await;
    assert!(html.contains("Add Mock"), "form should contain page title");
    assert!(html.contains("id_name"), "form should render name input");
}

#[actix_web::test]
async fn test_create_record() {
    let app = setup_app().await;
    let (cookie, token) = login_cookie(&app, "admin").await;

    let req = test::TestRequest::post()
        .uri("/admin/mock/new")
        .cookie(cookie)
        .set_form(std::collections::HashMap::from([
            ("name", "Test Record"),
            ("_csrf", token.as_str()),
        ]))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 302, "create should redirect to list");
    assert_eq!(resp.headers().get("location").unwrap(), "/admin/mock/");
}

#[actix_web::test]
async fn test_create_without_csrf_is_rejected() {
    let app = setup_app().await;
    let (cookie, _) = login_cookie(&app, "admin").await;

    let req = test::TestRequest::post()
        .uri("/admin/mock/new")
        .cookie(cookie)
        .set_form(std::collections::HashMap::from([("name", "No Token")]))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 403);
}

#[actix_web::test]
async fn test_edit_form_renders() {
    let app = setup_app().await;
    let (cookie, token) = login_cookie(&app, "admin").await;

    let req = test::TestRequest::post()
        .uri("/admin/mock/new")
        .cookie(cookie.clone())
        .set_form(std::collections::HashMap::from([
            ("name", "Editable Record"),
            ("_csrf", token.as_str()),
        ]))
        .to_request();
    test::call_service(&app, req).await;

    let html = page(&app, "/admin/mock/1", cookie).await;
    assert!(
        html.contains("Change Mock"),
        "form should contain page title"
    );
    assert!(
        html.contains("Editable Record"),
        "form should contain existing value"
    );
}

#[actix_web::test]
async fn test_update_record() {
    let app = setup_app().await;
    let (cookie, token) = login_cookie(&app, "admin").await;

    let req = test::TestRequest::post()
        .uri("/admin/mock/new")
        .cookie(cookie.clone())
        .set_form(std::collections::HashMap::from([
            ("name", "Old Name"),
            ("_csrf", token.as_str()),
        ]))
        .to_request();
    test::call_service(&app, req).await;

    let req = test::TestRequest::post()
        .uri("/admin/mock/1")
        .cookie(cookie)
        .set_form(std::collections::HashMap::from([
            ("name", "New Name"),
            ("_csrf", token.as_str()),
        ]))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 302, "update should redirect to list");
    assert_eq!(resp.headers().get("location").unwrap(), "/admin/mock/");
}

#[actix_web::test]
async fn test_delete_record() {
    let app = setup_app().await;
    let (cookie, token) = login_cookie(&app, "admin").await;

    let req = test::TestRequest::post()
        .uri("/admin/mock/new")
        .cookie(cookie.clone())
        .set_form(std::collections::HashMap::from([
            ("name", "To Delete"),
            ("_csrf", token.as_str()),
        ]))
        .to_request();
    test::call_service(&app, req).await;

    let req = test::TestRequest::post()
        .uri("/admin/mock/1/delete")
        .cookie(cookie)
        .set_form(std::collections::HashMap::from([("_csrf", token.as_str())]))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 302, "delete should redirect to list");
    assert_eq!(resp.headers().get("location").unwrap(), "/admin/mock/");
}
