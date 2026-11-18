use actix_session::Session;
use actix_web::{web, HttpResponse, Responder};
use serde::Deserialize;
use std::sync::Arc;
use tera::Tera;

use super::ctx;
use crate::auth::{csrf, verify_password, UserStore};
use crate::labels::AdminLabels;
use crate::resource::{AdminPrefix, AdminTitle};
use crate::theme::AdminTheme;

#[derive(Deserialize)]
pub struct PasswordForm {
    #[serde(rename = "_csrf", default)]
    pub csrf: String,
    pub current: String,
    pub new_password: String,
    pub confirm: String,
}

pub async fn page(
    session: Session,
    tmpl: web::Data<Tera>,
    title: web::Data<AdminTitle>,
    prefix: web::Data<AdminPrefix>,
    labels: web::Data<AdminLabels>,
    theme: web::Data<AdminTheme>,
) -> impl Responder {
    render(
        &session, &tmpl, &title, &prefix, &labels, &theme, None, false,
    )
}

pub async fn change(
    session: Session,
    store: web::Data<Arc<dyn UserStore>>,
    form: web::Form<PasswordForm>,
    tmpl: web::Data<Tera>,
    title: web::Data<AdminTitle>,
    prefix: web::Data<AdminPrefix>,
    labels: web::Data<AdminLabels>,
    theme: web::Data<AdminTheme>,
) -> impl Responder {
    if !csrf::verify(&session, &form.csrf) {
        return HttpResponse::Forbidden().body(labels.0.csrf_error);
    }
    let username = match ctx::current_user(&session) {
        Some(username) => username,
        None => {
            return HttpResponse::Found()
                .insert_header(("Location", format!("/{}/login", prefix.0)))
                .finish()
        }
    };

    if form.new_password != form.confirm {
        return render(
            &session,
            &tmpl,
            &title,
            &prefix,
            &labels,
            &theme,
            Some(labels.0.password_mismatch),
            false,
        );
    }

    let user = match store.find_by_username(&username).await {
        Ok(Some(user)) => user,
        _ => return HttpResponse::Unauthorized().finish(),
    };
    if !verify_password(&form.current, &user.password_hash).unwrap_or(false) {
        return render(
            &session,
            &tmpl,
            &title,
            &prefix,
            &labels,
            &theme,
            Some(labels.0.wrong_password),
            false,
        );
    }

    match store.update_password(&username, &form.new_password).await {
        Ok(()) => render(
            &session,
            &tmpl,
            &title,
            &prefix,
            &labels,
            &theme,
            Some(labels.0.password_changed),
            true,
        ),
        Err(e) => HttpResponse::InternalServerError().body(e.to_string()),
    }
}

#[allow(clippy::too_many_arguments)]
fn render(
    session: &Session,
    tmpl: &web::Data<Tera>,
    title: &web::Data<AdminTitle>,
    prefix: &web::Data<AdminPrefix>,
    labels: &web::Data<AdminLabels>,
    theme: &web::Data<AdminTheme>,
    message: Option<&'static str>,
    is_success: bool,
) -> HttpResponse {
    let user = ctx::current_user(session).unwrap_or_default();
    let mut context = ctx::page(title, labels, theme);
    ctx::paths(&mut context, prefix);
    ctx::csrf_token(&mut context, session);
    ctx::write_flag(&mut context, session);
    context.insert("page_title", &labels.0.change_password);
    context.insert("user", &user);
    context.insert("current_slug", &"password");
    context.insert("resources", &Vec::<crate::resource::ResourceInfo>::new());
    if let Some(text) = message {
        context.insert(if is_success { "success" } else { "error" }, &text);
    }
    let mut response = match tmpl.render("password.html", &context) {
        Ok(rendered) => HttpResponse::Ok().content_type("text/html").body(rendered),
        Err(e) => HttpResponse::InternalServerError().body(e.to_string()),
    };
    if !is_success && message.is_some() {
        *response.status_mut() = actix_web::http::StatusCode::BAD_REQUEST;
    }
    response
}
