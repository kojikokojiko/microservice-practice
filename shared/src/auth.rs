use axum::{
    async_trait,
    extract::FromRequestParts,
    http::{request::Parts, StatusCode},
};
use jsonwebtoken::{decode, DecodingKey, Validation};
use serde::Deserialize;
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    Admin,
    Teacher,
    Student,
}

impl fmt::Display for Role {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Role::Admin => write!(f, "admin"),
            Role::Teacher => write!(f, "teacher"),
            Role::Student => write!(f, "student"),
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct Claims {
    pub sub: String,
    pub role: Role,
    pub exp: i64,
    pub iss: Option<String>,
}

pub fn verify_jwt(token: &str, secret: &str) -> Result<Claims, jsonwebtoken::errors::Error> {
    let key = DecodingKey::from_secret(secret.as_ref());
    let mut validation = Validation::default();
    validation.validate_exp = true;
    let data = decode::<Claims>(token, &key, &validation)?;
    Ok(data.claims)
}

/// Extract Bearer token from Authorization header and verify; yields Claims.
pub struct AuthUser(pub Claims);

#[async_trait]
impl<S> FromRequestParts<S> for AuthUser
where
    S: Send + Sync,
{
    type Rejection = (StatusCode, &'static str);

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        let auth = parts
            .headers
            .get("Authorization")
            .and_then(|v| v.to_str().ok())
            .ok_or((StatusCode::UNAUTHORIZED, "Missing Authorization header"))?;
        let token = auth
            .strip_prefix("Bearer ")
            .ok_or((StatusCode::UNAUTHORIZED, "Invalid Authorization format"))?;
        let secret = std::env::var("JWT_SECRET").map_err(|_| {
            (StatusCode::INTERNAL_SERVER_ERROR, "JWT_SECRET not set")
        })?;
        let claims = verify_jwt(token, &secret)
            .map_err(|_| (StatusCode::UNAUTHORIZED, "Invalid or expired token"))?;
        Ok(AuthUser(claims))
    }
}
