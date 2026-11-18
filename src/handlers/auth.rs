use crate::auth::{csrf, verify_password, UserStore};
use crate::labels::AdminLabels;
use crate::resource::{AdminPrefix, AdminTitle};
use crate::theme::AdminTheme;
use actix_session::Session;
use actix_web::{web, HttpResponse, Responder};
use serde::Deserialize;
use std::sync::Arc;
use tera::Tera;

use super::ctx;

#[derive(Clone)]
#[deprecated(note = "use UserStore trait with a concrete implementation instead")]
pub struct SimpleAuth {
    pub username: String,
    pub password: String,
}

#[allow(deprecated)]
impl SimpleAuth {
    pub fn check(&self, username: &str, password: &str) -> bool {
        self.username == username && self.password == password
    }
}

#[derive(Deserialize)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
    #[serde(rename = "_csrf", default)]
    pub csrf: String,
}

pub async fn login_page(
    session: Session,
    tmpl: web::Data<Tera>,
    title: web::Data<AdminTitle>,
    prefix: web::Data<AdminPrefix>,
    labels: web::Data<AdminLabels>,
    theme: web::Data<AdminTheme>,
) -> impl Responder {
    render_login(&session, &tmpl, &title, &prefix, &labels, &theme, None)
}

pub async fn login(
    session: Session,
    store: web::Data<Arc<dyn UserStore>>,
    form: web::Form<LoginRequest>,
    tmpl: web::Data<Tera>,
    title: web::Data<AdminTitle>,
    prefix: web::Data<AdminPrefix>,
    labels: web::Data<AdminLabels>,
    theme: web::Data<AdminTheme>,
) -> impl Responder {
    if !csrf::verify(&session, &form.csrf) {
        return HttpResponse::Forbidden().body(labels.0.csrf_error);
    }

    let found = if form.username.contains('@') {
        store.find_by_email(&form.username).await
    } else {
        store.find_by_username(&form.username).await
    };

    if let Ok(Some(user)) = found {
        if verify_password(&form.password, &user.password_hash).unwrap_or(false) {
            if !csrf::remember_login(&session, &user.username, user.is_superuser) {
                return HttpResponse::InternalServerError().body("Session error");
            }
            return HttpResponse::Found()
                .insert_header(("Location", format!("/{}/", prefix.0)))
                .finish();
        }
    }

    render_login(
        &session,
        &tmpl,
        &title,
        &prefix,
        &labels,
        &theme,
        Some(labels.0.invalid_login),
    )
}

fn render_login(
    session: &Session,
    tmpl: &web::Data<Tera>,
    title: &web::Data<AdminTitle>,
    prefix: &web::Data<AdminPrefix>,
    labels: &web::Data<AdminLabels>,
    theme: &web::Data<AdminTheme>,
    error: Option<&'static str>,
) -> HttpResponse {
    let mut ctx = ctx::page(title, labels, theme);
    ctx::paths(&mut ctx, prefix);
    ctx::csrf_token(&mut ctx, session);
    ctx.insert("page_title", &labels.0.login);
    ctx.insert("resources", &Vec::<crate::resource::ResourceInfo>::new());
    if let Some(message) = error {
        ctx.insert("error", message);
    }
    match tmpl.render("login.html", &ctx) {
        Ok(rendered) => HttpResponse::Ok().content_type("text/html").body(rendered),
        Err(e) => HttpResponse::InternalServerError().body(e.to_string()),
    }
}

pub async fn logout(session: Session, prefix: web::Data<AdminPrefix>) -> impl Responder {
    session.purge();
    HttpResponse::Found()
        .insert_header(("Location", format!("/{}/login", prefix.0)))
        .finish()
}
