use axum::http::StatusCode;

#[utoipa::path(
    get,
    path = "/api/v1/health_check",
    tag = "v1",
    responses(
        (status = 200, description = "Service is healthy")
    )
)]
pub async fn health_check() -> StatusCode {
    StatusCode::OK
}
