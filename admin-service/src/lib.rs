mod routes;

use axum::{routing::get, Router};
use sqlx::PgPool;
use std::time::Duration;
use tower::ServiceBuilder;
use tower_http::timeout::TimeoutLayer;

pub fn app(pool: PgPool) -> Router {
    Router::new()
        .route("/health", get(routes::health))
        .route("/ready", get(routes::ready))
        .route("/api/admin/courses", axum::routing::post(routes::create_course))
        .route(
            "/api/admin/courses/:course_id",
            get(routes::get_course),
        )
        .with_state(AppState { pool })
        .layer(
            ServiceBuilder::new()
                .layer(tower_http::trace::TraceLayer::new_for_http())
                .layer(TimeoutLayer::new(Duration::from_secs(30))),
        )
}

#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
}
