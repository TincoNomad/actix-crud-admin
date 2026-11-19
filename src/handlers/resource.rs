use actix_session::Session;
use actix_web::{web, HttpResponse, Responder};
use serde_json::Value;
use std::collections::HashMap;
use tera::{Context, Tera};

use super::ctx;
use crate::audit::{AuditEntry, SharedAudit};
use crate::auth::csrf;
use crate::labels::AdminLabels;
use crate::registry::SharedRegistry;
use crate::resource::{AdminPrefix, AdminTitle};
use crate::theme::AdminTheme;
use crate::types::*;

/// Page size used by the CSV export.
const EXPORT_LIMIT: u64 = 10_000;

fn current_user(session: &Session, prefix: &AdminPrefix) -> Result<String, HttpResponse> {
    ctx::current_user(session).ok_or_else(|| {
        HttpResponse::Found()
            .insert_header(("Location", format!("/{}/login", prefix.0)))
            .finish()
    })
}

fn forbidden() -> HttpResponse {
    HttpResponse::Forbidden().body("Forbidden")
}

fn csrf_rejected() -> HttpResponse {
    HttpResponse::Forbidden().body("Invalid CSRF token")
}

fn filter_values(raw: &HashMap<String, String>) -> HashMap<String, String> {
    raw.iter()
        .filter_map(|(key, value)| {
            key.strip_prefix("filter_")
                .filter(|_| !value.is_empty())
                .map(|column| (column.to_string(), value.clone()))
        })
        .collect()
}

fn sidebar(ctx: &mut Context, registry: &SharedRegistry, prefix: &AdminPrefix) {
    let resources = registry
        .all()
        .iter()
        .map(|r| r.info(&prefix.0))
        .collect::<Vec<_>>();
    ctx.insert("resources", &resources);
}

/// Shared tail for every page: title, labels, theme, paths, user, permissions.
#[allow(clippy::too_many_arguments)]
fn base(
    session: &Session,
    user: &str,
    slug: &str,
    title: &AdminTitle,
    prefix: &AdminPrefix,
    labels: &AdminLabels,
    theme: &AdminTheme,
) -> Context {
    let mut ctx = ctx::page(title, labels, theme);
    ctx::paths(&mut ctx, prefix);
    ctx::csrf_token(&mut ctx, session);
    ctx::write_flag(&mut ctx, session);
    ctx.insert("user", user);
    ctx.insert("current_slug", slug);
    ctx
}

pub async fn list(
    session: Session,
    registry: web::Data<SharedRegistry>,
    tmpl: web::Data<Tera>,
    title: web::Data<AdminTitle>,
    prefix: web::Data<AdminPrefix>,
    labels: web::Data<AdminLabels>,
    theme: web::Data<AdminTheme>,
    path: web::Path<String>,
    query: web::Query<ListQuery>,
    raw: web::Query<HashMap<String, String>>,
) -> impl Responder {
    let slug = path.into_inner();
    let user = match current_user(&session, &prefix) {
        Ok(user) => user,
        Err(response) => return response,
    };
    let resource = match registry.get(&slug) {
        Some(resource) => resource,
        None => return HttpResponse::NotFound().finish(),
    };

    let filters = filter_values(&raw);
    let mut list_query = query.into_inner();
    list_query.filters = filters.clone();
    let search = list_query.search.clone().unwrap_or_default();
    let sort_by = list_query.sort_by.clone().unwrap_or_default();
    let sort_dir = list_query.sort_dir.clone().unwrap_or_default();

    let result = match resource.list(list_query).await {
        Ok(result) => result,
        Err(e) => return HttpResponse::InternalServerError().body(e.to_string()),
    };

    let mut ctx = base(&session, &user, &slug, &title, &prefix, &labels, &theme);
    ctx.insert("page_title", resource.plural_name());
    ctx.insert("resource", &resource.info(&prefix.0));
    ctx.insert("columns", &resource.list_columns());
    ctx.insert("rows", &result.rows);
    ctx.insert("page", &result.page);
    ctx.insert("per_page", &result.per_page);
    ctx.insert("total", &result.total);
    ctx.insert(
        "total_pages",
        &result.total.div_ceil(result.per_page.max(1)),
    );
    ctx.insert(
        "query",
        &serde_json::json!({
            "search": search,
            "sort_by": sort_by,
            "sort_dir": sort_dir,
        }),
    );
    ctx.insert("filters", &resource.filters().await);
    ctx.insert("active_filters", &filters);
    let mut link_params: Vec<String> = Vec::new();
    if !search.is_empty() {
        link_params.push(format!("search={}", urlencode(&search)));
    }
    if !sort_by.is_empty() {
        link_params.push(format!("sort_by={}", urlencode(&sort_by)));
    }
    if !sort_dir.is_empty() {
        link_params.push(format!("sort_dir={}", urlencode(&sort_dir)));
    }
    for (key, value) in &filters {
        link_params.push(format!("filter_{}={}", urlencode(key), urlencode(value)));
    }
    let link_query = if link_params.is_empty() {
        String::new()
    } else {
        format!("{}&", link_params.join("&"))
    };
    ctx.insert("link_query", &link_query);
    sidebar(&mut ctx, &registry, &prefix);

    match tmpl.render("list.html", &ctx) {
        Ok(rendered) => HttpResponse::Ok().content_type("text/html").body(rendered),
        Err(e) => HttpResponse::InternalServerError().body(e.to_string()),
    }
}

pub async fn new(
    session: Session,
    registry: web::Data<SharedRegistry>,
    tmpl: web::Data<Tera>,
    title: web::Data<AdminTitle>,
    prefix: web::Data<AdminPrefix>,
    labels: web::Data<AdminLabels>,
    theme: web::Data<AdminTheme>,
    path: web::Path<String>,
) -> impl Responder {
    let slug = path.into_inner();
    let user = match current_user(&session, &prefix) {
        Ok(user) => user,
        Err(response) => return response,
    };
    let resource = match registry.get(&slug) {
        Some(resource) => resource,
        None => return HttpResponse::NotFound().finish(),
    };

    let mut ctx = base(&session, &user, &slug, &title, &prefix, &labels, &theme);
    ctx.insert("page_title", &format!("Add {}", resource.name()));
    ctx.insert("resource", &resource.info(&prefix.0));
    ctx.insert("fields", &resource.form_fields_dynamic().await);
    ctx.insert("values", &HashMap::<String, Value>::new());
    ctx.insert("errors", &HashMap::<String, String>::new());
    ctx.insert("is_new", &true);
    sidebar(&mut ctx, &registry, &prefix);

    match tmpl.render("form.html", &ctx) {
        Ok(rendered) => HttpResponse::Ok().content_type("text/html").body(rendered),
        Err(e) => HttpResponse::InternalServerError().body(e.to_string()),
    }
}

pub async fn create(
    session: Session,
    registry: web::Data<SharedRegistry>,
    tmpl: web::Data<Tera>,
    title: web::Data<AdminTitle>,
    prefix: web::Data<AdminPrefix>,
    labels: web::Data<AdminLabels>,
    theme: web::Data<AdminTheme>,
    audit: web::Data<SharedAudit>,
    path: web::Path<String>,
    form: web::Form<HashMap<String, String>>,
) -> impl Responder {
    let slug = path.into_inner();
    let user = match current_user(&session, &prefix) {
        Ok(user) => user,
        Err(response) => return response,
    };
    let form_map = form.into_inner();
    if !csrf::verify(
        &session,
        form_map.get(csrf::FIELD).map(|s| s.as_str()).unwrap_or(""),
    ) {
        return csrf_rejected();
    }
    if !csrf::can_write(&session) {
        return forbidden();
    }
    let resource = match registry.get(&slug) {
        Some(resource) => resource,
        None => return HttpResponse::NotFound().finish(),
    };

    let continue_editing = form_map.contains_key("_continue");
    let add_another = form_map.contains_key("_addanother");
    let data = submitted(&form_map);

    match resource.validate(&data).await {
        Ok(()) => match resource.create(data).await {
            Ok(result) => {
                let id = result.get("id").map(json_id).unwrap_or_default();
                audit.record(entry(&user, "create", &slug, &id)).await.ok();
                let redirect = if continue_editing && !id.is_empty() {
                    format!("/{}/{}/{}", prefix.0, slug, id)
                } else if add_another {
                    format!("/{}/{}/new", prefix.0, slug)
                } else {
                    format!("/{}/{}/", prefix.0, slug)
                };
                HttpResponse::Found()
                    .insert_header(("Location", redirect))
                    .finish()
            }
            Err(e) => HttpResponse::InternalServerError().body(e.to_string()),
        },
        Err(errors) => {
            let mut ctx = base(&session, &user, &slug, &title, &prefix, &labels, &theme);
            ctx.insert("page_title", &format!("Add {}", resource.name()));
            ctx.insert("resource", &resource.info(&prefix.0));
            ctx.insert("fields", &resource.form_fields_dynamic().await);
            ctx.insert("values", &data);
            ctx.insert("errors", &errors);
            ctx.insert("is_new", &true);
            ctx.insert("record_id", &"");
            sidebar(&mut ctx, &registry, &prefix);
            match tmpl.render("form.html", &ctx) {
                Ok(rendered) => HttpResponse::BadRequest()
                    .content_type("text/html")
                    .body(rendered),
                Err(e) => HttpResponse::InternalServerError().body(e.to_string()),
            }
        }
    }
}

pub async fn edit(
    session: Session,
    registry: web::Data<SharedRegistry>,
    tmpl: web::Data<Tera>,
    title: web::Data<AdminTitle>,
    prefix: web::Data<AdminPrefix>,
    labels: web::Data<AdminLabels>,
    theme: web::Data<AdminTheme>,
    path: web::Path<(String, String)>,
) -> impl Responder {
    let (slug, id) = path.into_inner();
    let user = match current_user(&session, &prefix) {
        Ok(user) => user,
        Err(response) => return response,
    };
    let resource = match registry.get(&slug) {
        Some(resource) => resource,
        None => return HttpResponse::NotFound().finish(),
    };

    let value = match resource.get(&id).await {
        Ok(value) => value,
        Err(AdminError::NotFound) => return HttpResponse::NotFound().finish(),
        Err(e) => return HttpResponse::InternalServerError().body(e.to_string()),
    };
    let values: HashMap<String, Value> = match value.as_object() {
        Some(object) => object.iter().map(|(k, v)| (k.clone(), v.clone())).collect(),
        None => HashMap::new(),
    };

    let mut ctx = base(&session, &user, &slug, &title, &prefix, &labels, &theme);
    ctx.insert("page_title", &format!("Change {}", resource.name()));
    ctx.insert("resource", &resource.info(&prefix.0));
    ctx.insert("fields", &resource.form_fields_dynamic().await);
    ctx.insert("values", &values);
    ctx.insert("errors", &HashMap::<String, String>::new());
    ctx.insert("is_new", &false);
    ctx.insert("record_id", &id);
    sidebar(&mut ctx, &registry, &prefix);

    match tmpl.render("form.html", &ctx) {
        Ok(rendered) => HttpResponse::Ok().content_type("text/html").body(rendered),
        Err(e) => HttpResponse::InternalServerError().body(e.to_string()),
    }
}

pub async fn update(
    session: Session,
    registry: web::Data<SharedRegistry>,
    tmpl: web::Data<Tera>,
    title: web::Data<AdminTitle>,
    prefix: web::Data<AdminPrefix>,
    labels: web::Data<AdminLabels>,
    theme: web::Data<AdminTheme>,
    audit: web::Data<SharedAudit>,
    path: web::Path<(String, String)>,
    form: web::Form<HashMap<String, String>>,
) -> impl Responder {
    let (slug, id) = path.into_inner();
    let user = match current_user(&session, &prefix) {
        Ok(user) => user,
        Err(response) => return response,
    };
    let form_map = form.into_inner();
    if !csrf::verify(
        &session,
        form_map.get(csrf::FIELD).map(|s| s.as_str()).unwrap_or(""),
    ) {
        return csrf_rejected();
    }
    if !csrf::can_write(&session) {
        return forbidden();
    }
    let resource = match registry.get(&slug) {
        Some(resource) => resource,
        None => return HttpResponse::NotFound().finish(),
    };

    let continue_editing = form_map.contains_key("_continue");
    let add_another = form_map.contains_key("_addanother");
    let data = submitted(&form_map);

    match resource.validate(&data).await {
        Ok(()) => match resource.update(&id, data).await {
            Ok(_) => {
                audit.record(entry(&user, "update", &slug, &id)).await.ok();
                let redirect = if continue_editing {
                    format!("/{}/{}/{}", prefix.0, slug, id)
                } else if add_another {
                    format!("/{}/{}/new", prefix.0, slug)
                } else {
                    format!("/{}/{}/", prefix.0, slug)
                };
                HttpResponse::Found()
                    .insert_header(("Location", redirect))
                    .finish()
            }
            Err(e) => HttpResponse::InternalServerError().body(e.to_string()),
        },
        Err(errors) => {
            let mut ctx = base(&session, &user, &slug, &title, &prefix, &labels, &theme);
            ctx.insert("page_title", &format!("Change {}", resource.name()));
            ctx.insert("resource", &resource.info(&prefix.0));
            ctx.insert("fields", &resource.form_fields_dynamic().await);
            ctx.insert("values", &data);
            ctx.insert("errors", &errors);
            ctx.insert("is_new", &false);
            ctx.insert("record_id", &id);
            sidebar(&mut ctx, &registry, &prefix);
            match tmpl.render("form.html", &ctx) {
                Ok(rendered) => HttpResponse::BadRequest()
                    .content_type("text/html")
                    .body(rendered),
                Err(e) => HttpResponse::InternalServerError().body(e.to_string()),
            }
        }
    }
}

pub async fn delete(
    session: Session,
    registry: web::Data<SharedRegistry>,
    prefix: web::Data<AdminPrefix>,
    audit: web::Data<SharedAudit>,
    path: web::Path<(String, String)>,
    form: web::Form<HashMap<String, String>>,
) -> impl Responder {
    let (slug, id) = path.into_inner();
    let user = match current_user(&session, &prefix) {
        Ok(user) => user,
        Err(response) => return response,
    };
    let form_map = form.into_inner();
    if !csrf::verify(
        &session,
        form_map.get(csrf::FIELD).map(|s| s.as_str()).unwrap_or(""),
    ) {
        return csrf_rejected();
    }
    if !csrf::can_write(&session) {
        return forbidden();
    }
    let resource = match registry.get(&slug) {
        Some(resource) => resource,
        None => return HttpResponse::NotFound().finish(),
    };

    match resource.delete(&id).await {
        Ok(()) => {
            audit.record(entry(&user, "delete", &slug, &id)).await.ok();
            HttpResponse::Found()
                .insert_header(("Location", format!("/{}/{}/", prefix.0, slug)))
                .finish()
        }
        Err(AdminError::NotFound) => HttpResponse::NotFound().finish(),
        Err(e) => HttpResponse::InternalServerError().body(e.to_string()),
    }
}

/// Deletes every record selected with the `id_<primary key>` checkboxes.
pub async fn bulk(
    session: Session,
    registry: web::Data<SharedRegistry>,
    prefix: web::Data<AdminPrefix>,
    audit: web::Data<SharedAudit>,
    path: web::Path<String>,
    form: web::Form<HashMap<String, String>>,
) -> impl Responder {
    let slug = path.into_inner();
    let user = match current_user(&session, &prefix) {
        Ok(user) => user,
        Err(response) => return response,
    };
    let form_map = form.into_inner();
    if !csrf::verify(
        &session,
        form_map.get(csrf::FIELD).map(|s| s.as_str()).unwrap_or(""),
    ) {
        return csrf_rejected();
    }
    if !csrf::can_write(&session) {
        return forbidden();
    }
    let resource = match registry.get(&slug) {
        Some(resource) => resource,
        None => return HttpResponse::NotFound().finish(),
    };

    let ids: Vec<String> = form_map
        .keys()
        .filter_map(|key| key.strip_prefix("id_").map(|id| id.to_string()))
        .collect();
    let mut failed = false;
    for id in ids {
        match resource.delete(&id).await {
            Ok(()) => {
                audit.record(entry(&user, "delete", &slug, &id)).await.ok();
            }
            Err(_) => failed = true,
        }
    }

    let location = format!("/{}/{}/", prefix.0, slug);
    if failed {
        return HttpResponse::InternalServerError().body("Some records could not be deleted");
    }
    HttpResponse::Found()
        .insert_header(("Location", location))
        .finish()
}

/// Downloads the current selection (search, sort and filters) as CSV.
///
/// This is a human-readable report, not an import format: foreign keys are
/// exported as their labels (not ids) and booleans stay `true`/`false`.
pub async fn export(
    session: Session,
    registry: web::Data<SharedRegistry>,
    prefix: web::Data<AdminPrefix>,
    path: web::Path<String>,
    raw: web::Query<HashMap<String, String>>,
) -> impl Responder {
    let slug = path.into_inner();
    if current_user(&session, &prefix).is_err() {
        return HttpResponse::Found()
            .insert_header(("Location", format!("/{}/login", prefix.0)))
            .finish();
    }
    let resource = match registry.get(&slug) {
        Some(resource) => resource,
        None => return HttpResponse::NotFound().finish(),
    };

    let raw = raw.into_inner();
    let query = ListQuery {
        page: Some(1),
        per_page: Some(EXPORT_LIMIT),
        search: raw.get("search").cloned(),
        sort_by: raw.get("sort_by").cloned(),
        sort_dir: raw.get("sort_dir").cloned(),
        filters: filter_values(&raw),
    };
    let result = match resource.list(query).await {
        Ok(result) => result,
        Err(e) => return HttpResponse::InternalServerError().body(e.to_string()),
    };

    let body = to_csv(&resource.list_columns(), &result.rows);
    HttpResponse::Ok()
        .content_type("text/csv; charset=utf-8")
        .insert_header((
            "Content-Disposition",
            format!("attachment; filename=\"{}.csv\"", slug),
        ))
        .body(body)
}

/// Form keys are strings; submit buttons start with `_` and are skipped.
fn submitted(form: &HashMap<String, String>) -> HashMap<String, Value> {
    form.iter()
        .filter(|(key, _)| !key.starts_with('_'))
        .map(|(key, value)| (key.clone(), Value::String(value.clone())))
        .collect()
}

fn json_id(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        other => other.to_string(),
    }
}

fn entry(user: &str, action: &str, resource: &str, record_id: &str) -> AuditEntry {
    AuditEntry {
        timestamp: crate::auth::timestamp(),
        user: user.to_string(),
        action: action.to_string(),
        resource: resource.to_string(),
        record_id: record_id.to_string(),
    }
}

fn to_csv(columns: &[Column], rows: &[Value]) -> String {
    let header = columns
        .iter()
        .map(|c| csv_cell(&c.label))
        .collect::<Vec<_>>()
        .join(",");
    let mut out = format!("{}\n", header);
    for row in rows {
        let line = columns
            .iter()
            .map(|c| csv_cell(&cell_text(&row[&c.key])))
            .collect::<Vec<_>>()
            .join(",");
        out.push_str(&line);
        out.push('\n');
    }
    out
}

fn cell_text(value: &Value) -> String {
    match value {
        Value::Null => String::new(),
        Value::String(text) => text.clone(),
        other => other.to_string(),
    }
}

/// Minimal percent-encoding for query strings (no extra dependency).
fn urlencode(value: &str) -> String {
    value
        .bytes()
        .map(|byte| match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (byte as char).to_string()
            }
            b' ' => "+".to_string(),
            other => format!("%{:02X}", other),
        })
        .collect()
}

fn csv_cell(value: &str) -> String {
    if value.contains([',', '"', '\n']) {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filters_are_read_from_the_query_string() {
        let raw = HashMap::from([
            ("filter_active".to_string(), "true".to_string()),
            ("filter_category".to_string(), "".to_string()),
            ("search".to_string(), "tea".to_string()),
        ]);
        let filters = filter_values(&raw);
        assert_eq!(filters.get("active"), Some(&"true".to_string()));
        assert!(
            !filters.contains_key("category"),
            "empty filters are ignored"
        );
        assert!(!filters.contains_key("search"));
    }

    #[test]
    fn submit_buttons_are_not_treated_as_data() {
        let form = HashMap::from([
            ("name".to_string(), "Tea".to_string()),
            ("_save".to_string(), "".to_string()),
            ("_csrf".to_string(), "token".to_string()),
        ]);
        let data = submitted(&form);
        assert_eq!(data.get("name"), Some(&Value::String("Tea".into())));
        assert!(!data.contains_key("_save"));
        assert!(!data.contains_key("_csrf"));
    }

    #[test]
    fn urlencode_keeps_safe_characters() {
        assert_eq!(urlencode("tea"), "tea");
        assert_eq!(urlencode("green tea"), "green+tea");
        assert_eq!(urlencode("a&b=c"), "a%26b%3Dc");
    }

    #[test]
    fn csv_escapes_commas_and_quotes() {
        assert_eq!(csv_cell("plain"), "plain");
        assert_eq!(csv_cell("a,b"), "\"a,b\"");
        assert_eq!(csv_cell("say \"hi\""), "\"say \"\"hi\"\"\"");
    }

    #[test]
    fn export_header_follows_the_columns() {
        let columns = vec![
            Column::text("name", "Name"),
            Column::number("price", "Price"),
        ];
        let rows = vec![serde_json::json!({ "name": "Tea", "price": 3.5 })];
        let csv = to_csv(&columns, &rows);
        assert_eq!(csv, "Name,Price\nTea,3.5\n");
    }

    #[test]
    fn null_cells_are_empty() {
        let columns = vec![Column::text("name", "Name")];
        let rows = vec![serde_json::json!({ "name": null })];
        assert_eq!(to_csv(&columns, &rows), "Name\n\n");
    }
}
