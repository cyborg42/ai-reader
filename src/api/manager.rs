use crate::books::book::BookMeta;
use crate::books::library::Library;
use crate::student;
use crate::student::StudentInfo;
use argon2::{Argon2, PasswordHash, PasswordVerifier};
use axum::{
    Router,
    extract::{Json, Multipart, Query, State},
    response::IntoResponse,
    routing::{get, post},
};
use serde::Deserialize;
use sqlx::SqlitePool;
use std::sync::Arc;
use tower_sessions::Session;
use utoipa::ToSchema;

use super::upload_books;

#[derive(Deserialize, ToSchema)]
pub struct LoginRequest {
    pub email: String,
    pub password: String,
}

async fn manager_login(
    database: &SqlitePool,
    email: String,
    password: String,
) -> anyhow::Result<i64> {
    let manager = sqlx::query!("SELECT id, password FROM manager WHERE email = ?", email)
        .fetch_one(database)
        .await?;
    let parsed_hash = PasswordHash::new(&manager.password)
        .map_err(|e| anyhow::anyhow!("Failed to parse password hash: {}", e))?;
    Argon2::default()
        .verify_password(password.as_bytes(), &parsed_hash)
        .map_err(|e| anyhow::anyhow!("Failed to verify password: {}", e))?;
    Ok(manager.id)
}

#[utoipa::path(
    context_path = "/api/manager",
    path = "/login",
    method(post),
    request_body = LoginRequest,
    responses(
        (status = 200, description = "Login successful"),
        (status = 400, description = "Invalid credentials")
    )
)]
#[axum::debug_handler]
pub async fn login(
    State(library): State<Arc<Library>>,
    session: Session,
    Json(req): Json<LoginRequest>,
) -> impl IntoResponse {
    let db = &library.database;
    let LoginRequest { email, password } = req;
    match manager_login(db, email, password).await {
        Ok(id) => {
            session.insert("manager_id", id).await.unwrap();
            "Login successful".into_response()
        }
        Err(e) => (axum::http::StatusCode::BAD_REQUEST, e.to_string()).into_response(),
    }
}

#[utoipa::path(
    context_path = "/api/manager",
    path = "/logout",
    method(post),
    responses(
        (status = 200, description = "Logout successful")
    )
)]
pub async fn logout(session: Session) -> impl IntoResponse {
    let _ = session.delete().await;
    "Logout successful".into_response()
}

#[utoipa::path(
    context_path = "/api/manager",
    path = "/list_books",
    method(get),
    responses(
        (status = 200, description = "List of books", body = Vec<BookMeta>),
        (status = 401, description = "Unauthorized")
    )
)]
pub async fn list_books(
    State(library): State<Arc<Library>>,
    session: Session,
) -> impl IntoResponse {
    let Ok(Some(_)) = session.get::<i64>("manager_id").await else {
        return (axum::http::StatusCode::UNAUTHORIZED, ()).into_response();
    };
    match library.get_book_list(false).await {
        Ok(books) => Json(books).into_response(),
        Err(e) => (axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

#[utoipa::path(
    context_path = "/api/manager",
    path = "/upload_public_book",
    method(post),
    responses(
        (status = 200, description = "Book uploaded successfully", body = Vec<i64>),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn upload_public_book(
    State(library): State<Arc<Library>>,
    session: Session,
    multipart: Multipart,
) -> impl IntoResponse {
    let Ok(Some(_)) = session.get::<i64>("manager_id").await else {
        return (axum::http::StatusCode::UNAUTHORIZED, ()).into_response();
    };
    match upload_books(multipart, library).await {
        Ok(book_ids) => Json(book_ids).into_response(),
        Err(e) => (axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

#[utoipa::path(
    context_path = "/api/manager",
    path = "/remove_book",
    method(post),
    params(
        ("book_id" = i64, Query, description = "ID of the book to remove")
    ),
    responses(
        (status = 200, description = "Book removed successfully"),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn remove_book(
    State(library): State<Arc<Library>>,
    session: Session,
    Query(book_id): Query<i64>,
) -> impl IntoResponse {
    let Ok(Some(_)) = session.get::<i64>("manager_id").await else {
        return (axum::http::StatusCode::UNAUTHORIZED, ()).into_response();
    };
    match library.delete_book(book_id).await {
        Ok(_) => "Book removed successfully".into_response(),
        Err(e) => (axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

#[utoipa::path(
    context_path = "/api/manager",
    path = "/set_book_public",
    method(post),
    params(
        ("book_id" = i64, Query, description = "ID of the book to set public"),
        ("is_public" = bool, Query, description = "Whether to set the book as public")
    ),
    responses(
        (status = 200, description = "Book visibility updated successfully"),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn set_book_public(
    State(library): State<Arc<Library>>,
    session: Session,
    Query((book_id, is_public)): Query<(i64, bool)>,
) -> impl IntoResponse {
    let Ok(Some(_)) = session.get::<i64>("manager_id").await else {
        return (axum::http::StatusCode::UNAUTHORIZED, ()).into_response();
    };
    match library.set_book_public(book_id, is_public).await {
        Ok(_) => "Book visibility updated successfully".into_response(),
        Err(e) => (axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

#[utoipa::path(
    context_path = "/api/manager",
    path = "/list_students",
    method(get),
    responses(
        (status = 200, description = "List of students", body = Vec<StudentInfo>),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn list_students(
    State(library): State<Arc<Library>>,
    session: Session,
) -> impl IntoResponse {
    let db = &library.database;
    let Ok(Some(_)) = session.get::<i64>("manager_id").await else {
        return (axum::http::StatusCode::UNAUTHORIZED, ()).into_response();
    };
    match student::get_student_list(db).await {
        Ok(students) => Json(students).into_response(),
        Err(e) => (axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

pub fn get_manager_scope() -> Router<Arc<Library>> {
    Router::new().nest(
        "/manager",
        Router::new()
            .route("/login", post(login))
            .route("/logout", post(logout))
            .route("/list_books", get(list_books))
            .route("/upload_public_book", post(upload_public_book))
            .route("/remove_book", post(remove_book))
            .route("/set_book_public", post(set_book_public))
            .route("/list_students", get(list_students)),
    )
}
