use shared::{init_tracing, Config};
use teacher_service::app;
use std::net::SocketAddr;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let config = Config::from_env().map_err(|e| format!("config: {}", e))?;
    init_tracing();
    tracing::info!("starting {} on port {}", config.service_name, config.http_port);

    let pool = sqlx::postgres::PgPoolOptions::new()
        .connect(&config.database_url)
        .await?;

    let admin_base = std::env::var("ADMIN_SERVICE_URL")
        .unwrap_or_else(|_| "http://admin-service:8080".to_string());
    let teacher_base = std::env::var("TEACHER_SERVICE_URL")
        .unwrap_or_else(|_| "http://teacher-service:8080".to_string());
    let client = std::sync::Arc::new(shared::ServiceClient::new(admin_base, teacher_base));

    let app = app(pool, client);

    let addr = SocketAddr::from(([0, 0, 0, 0], config.http_port));
    axum::serve(
        tokio::net::TcpListener::bind(addr).await?,
        app.into_make_service(),
    )
    .await?;
    Ok(())
}
