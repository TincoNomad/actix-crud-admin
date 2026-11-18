//! Session-backed CSRF token and the two write-permission helpers.

use actix_session::Session;
use argon2::password_hash::rand_core::{OsRng, RngCore};

const TOKEN_KEY: &str = "csrf";
const SUPERUSER_KEY: &str = "admin_superuser";

/// Field name used by every `<form>` to carry the token.
pub const FIELD: &str = "_csrf";

/// Session key holding the logged in username.
pub const USER_KEY: &str = "admin_user";

/// Returns the session token, creating it on first use.
///
/// The cookie session store persists it on the response, so the token from
/// `GET /admin/login` is the one the browser sends back with the POST.
pub fn ensure_token(session: &Session) -> String {
    if let Ok(Some(token)) = session.get::<String>(TOKEN_KEY) {
        return token;
    }
    let mut bytes = [0u8; 32];
    OsRng.fill_bytes(&mut bytes);
    let token: String = bytes.iter().map(|b| format!("{:02x}", b)).collect();
    let _ = session.insert(TOKEN_KEY, &token);
    token
}

/// Compares the submitted field against the session token.
pub fn verify(session: &Session, submitted: &str) -> bool {
    let expected = session.get::<String>(TOKEN_KEY).ok().flatten();
    match expected {
        Some(token) => !token.is_empty() && token == submitted,
        None => false,
    }
}

/// Remembers the username and whether the user can write.
pub fn remember_login(session: &Session, username: &str, is_superuser: bool) -> bool {
    session.insert(USER_KEY, username).is_ok()
        && session.insert(SUPERUSER_KEY, is_superuser).is_ok()
}

/// Whether the current session is allowed to create, update or delete.
///
/// Fails closed: a session created before this flag existed is read-only.
pub fn can_write(session: &Session) -> bool {
    session
        .get::<bool>(SUPERUSER_KEY)
        .ok()
        .flatten()
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use actix_session::{storage::CookieSessionStore, SessionMiddleware};
    use actix_web::cookie::Key;
    use actix_web::{test, web, App, HttpResponse};

    async fn call<F>(request: actix_http::Request, handler: F) -> actix_web::dev::ServiceResponse
    where
        F: Fn(Session) -> actix_web::HttpResponse + Clone + 'static,
    {
        let app = test::init_service(
            App::new()
                .wrap(SessionMiddleware::new(
                    CookieSessionStore::default(),
                    Key::generate(),
                ))
                .route(
                    "/",
                    web::get().to(move |s: Session| {
                        let body = handler(s);
                        async move { body }
                    }),
                ),
        )
        .await;
        test::call_service(&app, request).await
    }

    #[actix_web::test]
    async fn token_is_stable_within_a_session() {
        let response = call(test::TestRequest::get().uri("/").to_request(), |session| {
            let first = ensure_token(&session);
            let second = ensure_token(&session);
            HttpResponse::Ok().body(format!("{}|{}", first, second))
        })
        .await;
        let body = test::read_body(response).await;
        let text = String::from_utf8_lossy(&body);
        let (first, second) = text.split_once('|').unwrap();
        assert_eq!(first, second);
        assert_eq!(first.len(), 64);
    }

    #[actix_web::test]
    async fn verify_rejects_missing_or_wrong_tokens() {
        let response = call(
            test::TestRequest::get()
                .uri("/")
                .insert_header(("Cookie", ""))
                .to_request(),
            |session| {
                let token = ensure_token(&session);
                let wrong = verify(&session, "nope");
                let empty = verify(&session, "");
                let right = verify(&session, &token);
                HttpResponse::Ok().body(format!("{}|{}|{}", wrong, empty, right))
            },
        )
        .await;
        let body = test::read_body(response).await;
        assert_eq!(String::from_utf8_lossy(&body), "false|false|true");
    }

    #[actix_web::test]
    async fn write_permission_fails_closed() {
        let response = call(test::TestRequest::get().uri("/").to_request(), |session| {
            let before = can_write(&session);
            remember_login(&session, "ana", false);
            let reader = can_write(&session);
            remember_login(&session, "root", true);
            let writer = can_write(&session);
            HttpResponse::Ok().body(format!("{}|{}|{}", before, reader, writer))
        })
        .await;
        let body = test::read_body(response).await;
        assert_eq!(String::from_utf8_lossy(&body), "false|false|true");
    }
}
