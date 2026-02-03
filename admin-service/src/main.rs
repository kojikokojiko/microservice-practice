use admin_service::app;
use shared::{init_tracing, Config};
use std::net::SocketAddr;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let config = Config::from_env().map_err(|e| format!("config: {}", e))?;
    init_tracing();
    tracing::info!("starting {} on port {}", config.service_name, config.http_port);

    let pool = sqlx::postgres::PgPoolOptions::new()
        .connect(&config.database_url)
        .await?;

    let app = app(pool);

    let addr = SocketAddr::from(([0, 0, 0, 0], config.http_port));
    axum::serve(
        tokio::net::TcpListener::bind(addr).await?,
        app.into_make_service(),
    )
    .await?;
    Ok(())
}
