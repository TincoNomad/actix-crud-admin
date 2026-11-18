use super::ctx;
use crate::labels::AdminLabels;
use crate::registry::SharedRegistry;
use crate::resource::{AdminPrefix, AdminTitle};
use crate::theme::AdminTheme;
use actix_session::Session;
use actix_web::{web, HttpResponse, Responder};
use tera::Tera;

pub async fn index(
    session: Session,
    registry: web::Data<SharedRegistry>,
    tmpl: web::Data<Tera>,
    title: web::Data<AdminTitle>,
    prefix: web::Data<AdminPrefix>,
    labels: web::Data<AdminLabels>,
    theme: web::Data<AdminTheme>,
) -> impl Responder {
    let user = match ctx::current_user(&session) {
        Some(user) => user,
        None => {
            return HttpResponse::Found()
                .insert_header(("Location", format!("/{}/login", prefix.0)))
                .finish()
        }
    };

    let mut context = ctx::page(&title, &labels, &theme);
    ctx::paths(&mut context, &prefix);
    ctx::write_flag(&mut context, &session);
    context.insert("page_title", &labels.0.dashboard);
    context.insert("user", &user);
    context.insert("current_slug", "");

    let resources: Vec<_> = registry
        .all()
        .iter()
        .map(|resource| {
            serde_json::json!({
                "name": resource.name(),
                "plural_name": resource.plural_name(),
                "slug": resource.slug(),
                "icon": resource.icon(),
                "can_create": resource.can_create(),
                "can_delete": resource.can_delete(),
                "path_list": format!("/{}/{}/", prefix.0, resource.slug()),
                "path_new": format!("/{}/{}/new", prefix.0, resource.slug()),
            })
        })
        .collect();
    context.insert("resources", &resources);

    match tmpl.render("dashboard.html", &context) {
        Ok(rendered) => HttpResponse::Ok().content_type("text/html").body(rendered),
        Err(e) => HttpResponse::InternalServerError().body(format!("Template error: {}", e)),
    }
}
