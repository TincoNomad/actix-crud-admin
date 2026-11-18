//! Shared bits every page context needs: labels, theme, paths, CSRF, permissions.

use actix_session::Session;
use tera::Context;

use crate::auth::csrf;
use crate::labels::AdminLabels;
use crate::resource::{AdminPrefix, AdminTitle};
use crate::theme::AdminTheme;

/// Builds a context with the site-wide values already inserted.
pub fn page(title: &AdminTitle, labels: &AdminLabels, theme: &AdminTheme) -> Context {
    let mut ctx = Context::new();
    ctx.insert("title", &title.0);
    ctx.insert("lang", &labels.0.locale);
    ctx.insert("labels", &labels.0);
    ctx.insert("theme_css", &theme.0.to_css());
    ctx
}

/// Inserts the absolute paths used by the sidebar and header.
pub fn paths(ctx: &mut Context, prefix: &AdminPrefix) {
    let base = prefix.0.trim_start_matches('/');
    ctx.insert("path_dashboard", &format!("/{}/", base));
    ctx.insert("path_logout", &format!("/{}/logout", base));
    ctx.insert("path_password", &format!("/{}/password", base));
    ctx.insert("path_audit", &format!("/{}/audit", base));
}

/// Inserts the CSRF token for the forms on this page.
pub fn csrf_token(ctx: &mut Context, session: &Session) {
    ctx.insert("csrf", &csrf::ensure_token(session));
}

/// Inserts whether the current user may create, update or delete.
pub fn write_flag(ctx: &mut Context, session: &Session) {
    ctx.insert("can_write", &csrf::can_write(session));
}

/// Username stored in the session.
pub fn current_user(session: &Session) -> Option<String> {
    session.get::<String>(csrf::USER_KEY).ok().flatten()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::labels::Labels;
    use crate::theme::Theme;

    #[test]
    fn page_carries_labels_and_theme() {
        let ctx = page(
            &AdminTitle("Mi panel".into()),
            &AdminLabels(Labels::spanish()),
            &AdminTheme(Theme::from_primary("#ff0000")),
        );
        let value: serde_json::Value = ctx.into_json();
        assert_eq!(value["title"], serde_json::json!("Mi panel"));
        assert_eq!(value["labels"]["save"], serde_json::json!("Guardar"));
        assert!(value["theme_css"]
            .as_str()
            .unwrap()
            .contains("--primary: #ff0000"));
    }

    #[test]
    fn paths_are_built_from_the_prefix() {
        let mut ctx = Context::new();
        paths(&mut ctx, &AdminPrefix("admin".into()));
        let value: serde_json::Value = ctx.into_json();
        assert_eq!(value["path_dashboard"], serde_json::json!("/admin/"));
        assert_eq!(value["path_audit"], serde_json::json!("/admin/audit"));
    }

    #[test]
    fn paths_tolerate_a_leading_slash() {
        let mut ctx = Context::new();
        paths(&mut ctx, &AdminPrefix("/admin".into()));
        let value: serde_json::Value = ctx.into_json();
        assert_eq!(value["path_password"], serde_json::json!("/admin/password"));
    }
}
