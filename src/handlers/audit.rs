use actix_session::Session;
use actix_web::{web, HttpResponse, Responder};
use tera::Tera;

use super::ctx;
use crate::audit::SharedAudit;
use crate::labels::AdminLabels;
use crate::resource::{AdminPrefix, AdminTitle};
use crate::theme::AdminTheme;

const LIMIT: usize = 200;

pub async fn index(
    session: Session,
    audit: web::Data<SharedAudit>,
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

    let entries = audit.recent(LIMIT).await.unwrap_or_default();

    let mut context = ctx::page(&title, &labels, &theme);
    ctx::paths(&mut context, &prefix);
    ctx::write_flag(&mut context, &session);
    context.insert("page_title", &labels.0.audit);
    context.insert("user", &user);
    context.insert("current_slug", &"audit");
    context.insert("entries", &entries);
    context.insert("resources", &Vec::<crate::resource::ResourceInfo>::new());

    match tmpl.render("audit.html", &context) {
        Ok(rendered) => HttpResponse::Ok().content_type("text/html").body(rendered),
        Err(e) => HttpResponse::InternalServerError().body(e.to_string()),
    }
}
