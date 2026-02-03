use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};
use shared::{AuthUser, Role};
use uuid::Uuid;

use crate::AppState;

#[derive(Deserialize)]
pub struct CreateCourseBody {
    pub name: String,
}

#[derive(Serialize)]
pub struct Course {
    pub id: Uuid,
    pub name: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

pub async fn health() -> &'static str {
    "ok"
}

pub async fn ready(State(state): State<AppState>) -> Result<&'static str, StatusCode> {
    sqlx::query("SELECT 1")
        .execute(&state.pool)
        .await
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    Ok("ok")
}

pub async fn create_course(
    State(state): State<AppState>,
    AuthUser(auth): AuthUser,
    Json(body): Json<CreateCourseBody>,
) -> Result<(StatusCode, Json<Course>), (StatusCode, &'static str)> {
    if auth.role != Role::Admin {
        return Err((StatusCode::FORBIDDEN, "admin role required"));
    }
    let id = Uuid::new_v4();
    let now = chrono::Utc::now();
    sqlx::query(
        r#"
        INSERT INTO admin.courses (id, name, created_at)
        VALUES ($1, $2, $3)
        "#,
    )
    .bind(id)
    .bind(&body.name)
    .bind(now)
    .execute(&state.pool)
    .await
    .map_err(|e| {
        tracing::error!("create_course: {}", e);
        (StatusCode::INTERNAL_SERVER_ERROR, "database error")
    })?;
    Ok((
        StatusCode::CREATED,
        Json(Course {
            id,
            name: body.name,
            created_at: now,
        }),
    ))
}

pub async fn get_course(
    State(state): State<AppState>,
    AuthUser(auth): AuthUser,
    Path(course_id): Path<Uuid>,
) -> Result<Json<Course>, (StatusCode, &'static str)> {
    // Admin: full access. Teacher: allowed for course-existence check (e.g. when creating assignment).
    if auth.role != Role::Admin && auth.role != Role::Teacher {
        return Err((StatusCode::FORBIDDEN, "admin or teacher role required"));
    }
    let row = sqlx::query_as::<_, (Uuid, String, chrono::DateTime<chrono::Utc>)>(
        "SELECT id, name, created_at FROM admin.courses WHERE id = $1",
    )
    .bind(course_id)
    .fetch_optional(&state.pool)
    .await
    .map_err(|e| {
        tracing::error!("get_course: {}", e);
        (StatusCode::INTERNAL_SERVER_ERROR, "database error")
    })?;
    let (id, name, created_at) = row.ok_or((StatusCode::NOT_FOUND, "course not found"))?;
    Ok(Json(Course {
        id,
        name,
        created_at,
    }))
}
