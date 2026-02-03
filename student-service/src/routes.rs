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
pub struct CreateSubmissionBody {
    pub content: Option<String>,
}

#[derive(Serialize)]
pub struct Submission {
    pub id: Uuid,
    pub assignment_id: Uuid,
    pub student_id: String,
    pub content: Option<String>,
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

pub async fn create_submission(
    State(state): State<AppState>,
    headers: HeaderMap,
    AuthUser(auth): AuthUser,
    Path(assignment_id): Path<Uuid>,
    Json(body): Json<CreateSubmissionBody>,
) -> Result<(StatusCode, Json<Submission>), (StatusCode, &'static str)> {
    if auth.role != Role::Student {
        return Err((StatusCode::FORBIDDEN, "student role required"));
    }
    let bearer = headers.get(AUTHORIZATION).and_then(|v| v.to_str().ok());
    // Verify assignment exists via teacher-service (K8s DNS)
    let path = format!("/api/teacher/assignments/{}", assignment_id);
    let res = state.http_client.get_teacher(&path, bearer).await.map_err(|e| {
        tracing::warn!("teacher-service call failed: {}", e);
        (StatusCode::BAD_GATEWAY, "assignment service unavailable")
    })?;
    if !res.status().is_success() {
        return Err((StatusCode::NOT_FOUND, "assignment not found"));
    }

    let id = Uuid::new_v4();
    let now = chrono::Utc::now();
    let student_id = auth.sub.clone();
    sqlx::query(
        r#"
        INSERT INTO student.submissions (id, assignment_id, student_id, content, created_at)
        VALUES ($1, $2, $3, $4, $5)
        "#,
    )
    .bind(id)
    .bind(assignment_id)
    .bind(&student_id)
    .bind(body.content.as_deref())
    .bind(now)
    .execute(&state.pool)
    .await
    .map_err(|e| {
        tracing::error!("create_submission: {}", e);
        (StatusCode::INTERNAL_SERVER_ERROR, "database error")
    })?;
    Ok((
        StatusCode::CREATED,
        Json(Submission {
            id,
            assignment_id,
            student_id,
            content: body.content,
            created_at: now,
        }),
    ))
}
