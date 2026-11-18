pub mod csrf;
pub mod json_store;
pub mod middleware;
pub mod store;

pub use json_store::JsonUserStore;
pub use middleware::RequireAuth;
pub use store::{hash_password, verify_password, AuthError, User, UserStore};

/// Helper to generate a unique user ID (nanosecond timestamp as hex).
pub use store::generate_id;

/// Helper to get current timestamp in ISO 8601 format.
pub use store::timestamp;

/// Session middleware with production-safe cookie flags.
///
/// Same as building `SessionMiddleware` by hand, but `HttpOnly`, `Secure` and
/// `SameSite::Lax` are always on. The key must be stable across restarts, or
/// every deploy logs everybody out.
pub fn session_middleware(
    key: actix_web::cookie::Key,
) -> actix_session::SessionMiddleware<actix_session::storage::CookieSessionStore> {
    actix_session::SessionMiddleware::builder(
        actix_session::storage::CookieSessionStore::default(),
        key,
    )
    .cookie_secure(true)
    .cookie_http_only(true)
    .cookie_same_site(actix_web::cookie::SameSite::Lax)
    .build()
}
