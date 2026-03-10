use leptos::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AuthUserInfo {
    pub id: String,
    pub email: String,
    pub role: String,
}

#[server]
pub async fn login(email: String, password: String) -> Result<bool, ServerFnError> {
    let pool = crate::db::db().await?;

    let user = crate::db::get_user_by_email(&pool, &email)
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))?;

    let user = match user {
        Some(u) => u,
        None => {
            tracing::warn!(email = %email, "Login attempt for non-existent user");
            return Ok(false);
        }
    };

    let valid =
        crate::auth::verify_password(&password, &user.password_hash).map_err(ServerFnError::new)?;

    if !valid {
        tracing::warn!(email = %email, "Login attempt with invalid password");
        return Ok(false);
    }

    let session_id = uuid::Uuid::new_v4();
    let expires_at = chrono::Utc::now() + chrono::Duration::days(7);
    crate::db::create_session(&pool, session_id, user.id, expires_at)
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))?;

    let response = expect_context::<leptos_axum::ResponseOptions>();
    response.insert_header(
        axum::http::header::SET_COOKIE,
        axum::http::HeaderValue::from_str(&format!(
            "session_id={}; Path=/; HttpOnly; SameSite=Lax; Max-Age=604800",
            session_id
        ))
        .unwrap(),
    );

    tracing::info!(email = %email, user_id = %user.id, "User logged in successfully");
    Ok(true)
}

#[server]
pub async fn logout() -> Result<(), ServerFnError> {
    let pool = crate::db::db().await?;

    // Try to extract and delete the session
    if let Some(session_id) = extract_session_id().await {
        let _ = crate::db::delete_session(&pool, session_id).await;
        tracing::info!(session_id = %session_id, "User logged out");
    }

    let response = expect_context::<leptos_axum::ResponseOptions>();
    response.insert_header(
        axum::http::header::SET_COOKIE,
        axum::http::HeaderValue::from_str("session_id=; Path=/; HttpOnly; SameSite=Lax; Max-Age=0")
            .unwrap(),
    );

    Ok(())
}

#[server]
pub async fn get_current_user() -> Result<Option<AuthUserInfo>, ServerFnError> {
    let user = crate::auth::get_current_user().await?;
    Ok(user.map(|u| AuthUserInfo {
        id: u.id.to_string(),
        email: u.email,
        role: u.role,
    }))
}

#[cfg(feature = "ssr")]
async fn extract_session_id() -> Option<uuid::Uuid> {
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

#[component]
pub fn LoginPage() -> impl IntoView {
    let toast = crate::components::use_toast();
    let email = RwSignal::new(String::new());
    let password = RwSignal::new(String::new());

    let login_action = ServerAction::<Login>::new();
    let login_pending = login_action.pending();
    let login_result = login_action.value();

    Effect::new(move || {
        if let Some(result) = login_result.get() {
            match result {
                Ok(true) => {
                    // Redirect to admin page
                    #[cfg(feature = "hydrate")]
                    {
                        let window = leptos::web_sys::window().unwrap();
                        let _ = window.location().set_href("/admin");
                    }
                }
                Ok(false) => {
                    toast.error("Invalid email or password".to_string());
                }
                Err(e) => {
                    toast.error(format!("Login failed: {}", e));
                }
            }
        }
    });

    let on_submit = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        login_action.dispatch(Login {
            email: email.get_untracked(),
            password: password.get_untracked(),
        });
    };

    view! {
        <div class="login-page">
            <div class="login-card">
                <h1>"Admin Login"</h1>
                <form on:submit=on_submit>
                    <div class="form-field">
                        <label for="email">"Email"</label>
                        <input
                            id="email"
                            type="email"
                            placeholder="admin@example.com"
                            required
                            prop:value=move || email.get()
                            on:input=move |e| email.set(event_target_value(&e))
                        />
                    </div>
                    <div class="form-field">
                        <label for="password">"Password"</label>
                        <input
                            id="password"
                            type="password"
                            placeholder="Password"
                            required
                            prop:value=move || password.get()
                            on:input=move |e| password.set(event_target_value(&e))
                        />
                    </div>
                    <button type="submit" disabled=move || login_pending.get()>
                        {move || if login_pending.get() { "Signing in..." } else { "Sign In" }}
                    </button>
                </form>
            </div>
        </div>
    }
}
