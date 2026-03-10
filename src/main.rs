#[cfg(feature = "ssr")]
#[tokio::main]
async fn main() {
    use axum::routing::get;
    use axum::Router;
    use gurujisivananda::app::*;
    use gurujisivananda::configuration;
    use gurujisivananda::routes::{
        download_track, health_check, list_tracks, stream_track, ApiDoc, AppState,
    };
    use gurujisivananda::telemetry::{get_subscriber, init_subscriber};
    use leptos::prelude::*;
    use leptos_axum::{generate_route_list, LeptosRoutes};
    use sqlx::postgres::PgPoolOptions;
    use tower_http::request_id::{MakeRequestUuid, PropagateRequestIdLayer, SetRequestIdLayer};
    use tower_http::trace::TraceLayer;
    use utoipa::OpenApi;
    use utoipa_swagger_ui::SwaggerUi;

    let subscriber = get_subscriber("gurujisivananda".into(), "info".into(), std::io::stdout);
    init_subscriber(subscriber);

    let app_config = configuration::get_configuration().expect("Failed to read configuration");

    let pool = PgPoolOptions::new().connect_lazy_with(app_config.database.connection_options());

    sqlx::migrate!()
        .run(&pool)
        .await
        .expect("Could not run database migrations");

    let conf = get_configuration(None).unwrap();
    let addr = conf.leptos_options.site_addr;
    let leptos_options = conf.leptos_options;
    let routes = generate_route_list(App);

    let (s3_client, s3_bucket) = if let Some(s3_settings) = &app_config.audio.s3 {
        let client = gurujisivananda::storage::build_s3_client(s3_settings).await;
        tracing::info!("S3 storage enabled (bucket: {})", s3_settings.bucket);
        (Some(client), Some(s3_settings.bucket.clone()))
    } else {
        tracing::info!("Using local file storage");
        (None, None)
    };

    let app_state = AppState {
        pool: pool.clone(),
        audio_storage_path: app_config.audio.storage_path.clone(),
        s3_client,
        s3_bucket,
    };

    let track_routes = Router::new()
        .route("/tracks", get(list_tracks))
        .route("/tracks/{id}/stream", get(stream_track))
        .route("/tracks/{id}/download", get(download_track))
        .with_state(app_state);

    let api_routes = Router::new()
        .route("/health_check", get(health_check))
        .merge(track_routes);

    let app = Router::new()
        .nest("/api/v1", api_routes)
        .merge(SwaggerUi::new("/api/swagger-ui").url("/api/openapi.json", ApiDoc::openapi()))
        .leptos_routes_with_context(
            &leptos_options,
            routes,
            {
                let pool = pool.clone();
                move || provide_context(pool.clone())
            },
            {
                let leptos_options = leptos_options.clone();
                move || shell(leptos_options.clone())
            },
        )
        .fallback(leptos_axum::file_and_error_handler(shell))
        .layer(
            TraceLayer::new_for_http().make_span_with(|request: &axum::http::Request<_>| {
                let request_id = request
                    .headers()
                    .get("x-request-id")
                    .and_then(|v| v.to_str().ok())
                    .unwrap_or("unknown");
                tracing::info_span!(
                    "http_request",
                    method = %request.method(),
                    uri = %request.uri(),
                    request_id = %request_id,
                )
            }),
        )
        .layer(SetRequestIdLayer::x_request_id(MakeRequestUuid))
        .layer(PropagateRequestIdLayer::x_request_id())
        .with_state(leptos_options);

    tracing::info!("listening on http://{}", &addr);
    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
    axum::serve(listener, app.into_make_service())
        .await
        .unwrap();
}

#[cfg(not(feature = "ssr"))]
pub fn main() {
    // no client-side main function
    // unless we want this to work with e.g., Trunk for pure client-side testing
    // see lib.rs for hydration function instead
}
