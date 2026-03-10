use argon2::password_hash::SaltString;
use argon2::{Argon2, PasswordHash, PasswordHasher, PasswordVerifier};
use rand::rngs::OsRng;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AuthUser {
    pub id: uuid::Uuid,
    pub email: String,
    pub role: String,
}

pub fn hash_password(password: &str) -> Result<String, String> {
    let salt = SaltString::generate(&mut OsRng);
    let argon2 = Argon2::default();
    argon2
        .hash_password(password.as_bytes(), &salt)
        .map(|h| h.to_string())
        .map_err(|e| format!("Failed to hash password: {}", e))
}

pub fn verify_password(password: &str, hash: &str) -> Result<bool, String> {
    let parsed = PasswordHash::new(hash).map_err(|e| format!("Invalid hash: {}", e))?;
    Ok(Argon2::default()
        .verify_password(password.as_bytes(), &parsed)
        .is_ok())
}

/// Extract the current authenticated user from the request cookie in a server function context.
pub async fn get_current_user() -> Result<Option<AuthUser>, leptos::prelude::ServerFnError> {
    let pool = crate::db::db().await?;

    let session_id = match extract_session_cookie().await {
        Some(id) => id,
        None => return Ok(None),
    };

    let row = sqlx::query_as::<_, (uuid::Uuid, String, String, chrono::DateTime<chrono::Utc>)>(
        "SELECT u.id, u.email, u.role, s.expires_at \
         FROM sessions s JOIN users u ON s.user_id = u.id \
         WHERE s.id = $1",
    )
    .bind(session_id)
    .fetch_optional(&pool)
    .await
    .map_err(|e| leptos::prelude::ServerFnError::new(e.to_string()))?;

    match row {
        Some((id, email, role, expires_at)) => {
            if expires_at < chrono::Utc::now() {
                // Session expired, clean it up
                let _ = sqlx::query("DELETE FROM sessions WHERE id = $1")
                    .bind(session_id)
                    .execute(&pool)
                    .await;
                Ok(None)
            } else {
                Ok(Some(AuthUser { id, email, role }))
            }
        }
        None => Ok(None),
    }
}

/// Require that the current user is an admin. Returns the user or an error.
pub async fn require_admin() -> Result<AuthUser, leptos::prelude::ServerFnError> {
    match get_current_user().await? {
        Some(user) if user.role == "admin" => Ok(user),
        Some(_) => Err(leptos::prelude::ServerFnError::new("Admin access required")),
        None => Err(leptos::prelude::ServerFnError::new("Not authenticated")),
    }
}

/// Seed an admin user on startup if env vars are set and admin doesn't exist.
pub async fn seed_admin_if_needed(pool: &sqlx::PgPool) {
    let email = match std::env::var("APP_ADMIN_EMAIL") {
        Ok(e) if !e.is_empty() => e,
        _ => return,
    };
    let password = match std::env::var("APP_ADMIN_PASSWORD") {
        Ok(p) if !p.is_empty() => p,
        _ => return,
    };

    let exists =
        sqlx::query_scalar::<_, bool>("SELECT EXISTS(SELECT 1 FROM users WHERE email = $1)")
            .bind(&email)
            .fetch_one(pool)
            .await;

    match exists {
        Ok(true) => {
            tracing::info!("Admin user already exists: {}", email);
        }
        Ok(false) => {
            let password_hash = match hash_password(&password) {
                Ok(h) => h,
                Err(e) => {
                    tracing::error!("Failed to hash admin password: {}", e);
                    return;
                }
            };
            let result = sqlx::query(
                "INSERT INTO users (id, email, password_hash, role) VALUES ($1, $2, $3, 'admin')",
            )
            .bind(uuid::Uuid::new_v4())
            .bind(&email)
            .bind(&password_hash)
            .execute(pool)
            .await;

            match result {
                Ok(_) => tracing::info!("Admin user seeded: {}", email),
                Err(e) => tracing::error!("Failed to seed admin user: {}", e),
            }
        }
        Err(e) => {
            tracing::warn!(
                "Could not check for admin user (table may not exist yet): {}",
                e
            );
        }
    }
}

async fn extract_session_cookie() -> Option<uuid::Uuid> {
    use axum::http::request::Parts;
    let parts: Parts = leptos_axum::extract().await.ok()?;
    let cookie_header = parts.headers.get("cookie")?.to_str().ok()?;

    for cookie in cookie_header.split(';') {
        let cookie = cookie.trim();
        if let Some(value) = cookie.strip_prefix("session_id=") {
            return uuid::Uuid::parse_str(value).ok();
        }
    }
    None
}
