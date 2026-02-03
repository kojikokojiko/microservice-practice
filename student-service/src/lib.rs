mod routes;

use axum::{routing::get, Router};
use shared::ServiceClient;
use sqlx::PgPool;
use std::time::Duration;
use tower::ServiceBuilder;
use tower_http::timeout::TimeoutLayer;

#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub http_client: std::sync::Arc<ServiceClient>,
}

pub fn app(pool: PgPool, http_client: std::sync::Arc<ServiceClient>) -> Router {
    Router::new()
        .route("/health", get(routes::health))
        .route("/ready", get(routes::ready))
        .route(
            "/api/student/assignments/:assignment_id/submissions",
            axum::routing::post(routes::create_submission),
        )
        .with_state(AppState { pool, http_client })
        .layer(
            ServiceBuilder::new()
                .layer(tower_http::trace::TraceLayer::new_for_http())
                .layer(TimeoutLayer::new(Duration::from_secs(30))),
        )
}
