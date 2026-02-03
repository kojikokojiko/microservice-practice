use std::env;

#[derive(Clone, Debug)]
pub struct Config {
    pub service_name: String,
    pub http_port: u16,
    pub database_url: String,
    pub jwt_secret: String,
    pub rust_log: String,
}

impl Config {
    pub fn from_env() -> Result<Self, env::VarError> {
        Ok(Self {
            service_name: env::var("SERVICE_NAME").unwrap_or_else(|_| "local".to_string()),
            http_port: env::var("HTTP_PORT")
                .unwrap_or_else(|_| "8080".to_string())
                .parse()
                .unwrap_or(8080),
            database_url: env::var("DATABASE_URL")?,
            jwt_secret: env::var("JWT_SECRET")?,
            rust_log: env::var("RUST_LOG").unwrap_or_else(|_| "info".to_string()),
        })
    }
}
