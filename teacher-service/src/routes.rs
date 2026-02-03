use axum::{
    extract::{Path, State},
    http::{header::AUTHORIZATION, HeaderMap, StatusCode},
    Json,
};
use serde::{Deserialize, Serialize};
use shared::{AuthUser, Role};
use uuid::Uuid;

use crate::AppState;

#[derive(Deserialize)]
pub struct CreateAssignmentBody {
    pub title: String,
}

#[derive(Serialize)]
pub struct Assignment {
    pub id: Uuid,
    pub course_id: Uuid,
    pub title: String,
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

pub async fn create_assignment(
    State(state): State<AppState>,
    headers: HeaderMap,
    AuthUser(auth): AuthUser,
    Path(course_id): Path<Uuid>,
    Json(body): Json<CreateAssignmentBody>,
) -> Result<(StatusCode, Json<Assignment>), (StatusCode, &'static str)> {
    if auth.role != Role::Teacher {
        return Err((StatusCode::FORBIDDEN, "teacher role required"));
    }
    let bearer = headers.get(AUTHORIZATION).and_then(|v| v.to_str().ok());
    // Verify course exists via admin-service (K8s DNS)
    let path = format!("/api/admin/courses/{}", course_id);
    let res = state.http_client.get_admin(&path, bearer).await.map_err(|e| {
        tracing::warn!("admin-service call failed: {}", e);
        (StatusCode::BAD_GATEWAY, "course service unavailable")
    })?;
    if !res.status().is_success() {
        return Err((StatusCode::NOT_FOUND, "course not found"));
    }

    let id = Uuid::new_v4();
    let now = chrono::Utc::now();
    sqlx::query(
        r#"
        INSERT INTO teacher.assignments (id, course_id, title, created_at)
        VALUES ($1, $2, $3, $4)
        "#,
    )
    .bind(id)
    .bind(course_id)
    .bind(&body.title)
    .bind(now)
    .execute(&state.pool)
    .await
    .map_err(|e| {
        tracing::error!("create_assignment: {}", e);
        (StatusCode::INTERNAL_SERVER_ERROR, "database error")
    })?;
    Ok((
        StatusCode::CREATED,
        Json(Assignment {
            id,
            course_id,
            title: body.title,
            created_at: now,
        }),
    ))
}

pub async fn get_assignment(
    State(state): State<AppState>,
    AuthUser(auth): AuthUser,
    Path(assignment_id): Path<Uuid>,
) -> Result<Json<Assignment>, (StatusCode, &'static str)> {
    // Teacher: full access. Student: allowed for assignment-existence check (e.g. when submitting).
    if auth.role != Role::Teacher && auth.role != Role::Student {
        return Err((StatusCode::FORBIDDEN, "teacher or student role required"));
    }
    let row = sqlx::query_as::<_, (Uuid, Uuid, String, chrono::DateTime<chrono::Utc>)>(
        "SELECT id, course_id, title, created_at FROM teacher.assignments WHERE id = $1",
    )
    .bind(assignment_id)
    .fetch_optional(&state.pool)
    .await
    .map_err(|e| {
        tracing::error!("get_assignment: {}", e);
        (StatusCode::INTERNAL_SERVER_ERROR, "database error")
    })?;
    let (id, course_id, title, created_at) =
        row.ok_or((StatusCode::NOT_FOUND, "assignment not found"))?;
    Ok(Json(Assignment {
        id,
        course_id,
        title,
        created_at,
    }))
}
