use std::sync::Arc;

use actix_web::{HttpResponse, Responder, delete, get, post, web};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use utoipa::ToSchema;

use crate::{
    book::{book::Book, library::Library},
    student::{
        StudentInfo, create_student, delete_student, delete_student_book, get_student_books,
        get_student_list,
    },
    teacher::TeacherAgent,
};

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct UploadBookRequest {
    pub file_path: String,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct CreateStudentRequest {
    pub name: String,
    pub email: String,
    pub password: String,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ErrorResponse {
    pub error: String,
}

/// List all books
#[utoipa::path(
    get,
    path = "/api/books",
    responses(
        (status = 200, description = "List of books", body = Vec<Book>),
        (status = 500, description = "Internal server error", body = ErrorResponse),
    )
)]
#[get("/books")]
pub async fn list_books(library: web::Data<Arc<Library>>) -> impl Responder {
    match library.get_book_list().await {
        Ok(books) => HttpResponse::Ok().json(books),
        Err(e) => HttpResponse::InternalServerError().json(ErrorResponse {
            error: e.to_string(),
        }),
    }
}

/// Upload a new book
#[utoipa::path(
    post,
    path = "/api/books",
    request_body = UploadBookRequest,
    responses(
        (status = 200, description = "Book uploaded successfully"),
        (status = 500, description = "Internal server error", body = ErrorResponse),
    )
)]
#[post("/books")]
pub async fn upload_book(
    library: web::Data<Arc<Library>>,
    req: web::Json<UploadBookRequest>,
) -> impl Responder {
    match library.upload_book(&req.file_path).await {
        Ok(_) => HttpResponse::Ok().json("Book uploaded successfully"),
        Err(e) => HttpResponse::InternalServerError().json(ErrorResponse {
            error: e.to_string(),
        }),
    }
}

/// Delete a book
#[utoipa::path(
    delete,
    path = "/api/books/{id}",
    params(
        ("id" = i64, Path, description = "Book ID"),
    ),
    responses(
        (status = 200, description = "Book deleted successfully"),
        (status = 500, description = "Internal server error", body = ErrorResponse),
    )
)]
#[delete("/books/{id}")]
pub async fn delete_book(library: web::Data<Arc<Library>>, id: web::Path<i64>) -> impl Responder {
    match library.delete_book(*id).await {
        Ok(_) => HttpResponse::Ok().json("Book deleted successfully"),
        Err(e) => HttpResponse::InternalServerError().json(ErrorResponse {
            error: e.to_string(),
        }),
    }
}

/// List all students
#[utoipa::path(
    get,
    path = "/api/students",
    responses(
        (status = 200, description = "List of students", body = Vec<StudentInfo>),
        (status = 500, description = "Internal server error", body = ErrorResponse),
    )
)]
#[get("/students")]
pub async fn list_students(db: web::Data<SqlitePool>) -> impl Responder {
    match get_student_list(&db).await {
        Ok(students) => HttpResponse::Ok().json(students),
        Err(e) => HttpResponse::InternalServerError().json(ErrorResponse {
            error: e.to_string(),
        }),
    }
}

/// Create a new student
#[utoipa::path(
    post,
    path = "/api/students",
    request_body = CreateStudentRequest,
    responses(
        (status = 200, description = "Student created successfully", body = i64),
        (status = 500, description = "Internal server error", body = ErrorResponse),
    )
)]
#[post("/students")]
pub async fn create_student_endpoint(
    db: web::Data<SqlitePool>,
    req: web::Json<CreateStudentRequest>,
) -> impl Responder {
    match create_student(
        &db,
        req.name.clone(),
        req.email.clone(),
        req.password.clone(),
    )
    .await
    {
        Ok(id) => HttpResponse::Ok().json(id),
        Err(e) => HttpResponse::InternalServerError().json(ErrorResponse {
            error: e.to_string(),
        }),
    }
}

/// Delete a student
#[utoipa::path(
    delete,
    path = "/api/students/{id}",
    params(
        ("id" = i64, Path, description = "Student ID"),
    ),
    responses(
        (status = 200, description = "Student deleted successfully"),
        (status = 500, description = "Internal server error", body = ErrorResponse),
    )
)]
#[delete("/students/{id}")]
pub async fn delete_student_endpoint(
    db: web::Data<SqlitePool>,
    id: web::Path<i64>,
) -> impl Responder {
    match delete_student(&db, *id).await {
        Ok(_) => HttpResponse::Ok().json("Student deleted successfully"),
        Err(e) => HttpResponse::InternalServerError().json(ErrorResponse {
            error: e.to_string(),
        }),
    }
}

/// List all books for a student
#[utoipa::path(
    get,
    path = "/api/students/{id}/books",
    params(
        ("id" = i64, Path, description = "Student ID"),
    ),
    responses(
        (status = 200, description = "List of books", body = Vec<Book>),
        (status = 500, description = "Internal server error", body = ErrorResponse),
    )
)]
#[get("/students/{id}/books")]
pub async fn list_student_books(db: web::Data<SqlitePool>, id: web::Path<i64>) -> impl Responder {
    match get_student_books(&db, *id).await {
        Ok(books) => HttpResponse::Ok().json(books),
        Err(e) => HttpResponse::InternalServerError().json(ErrorResponse {
            error: e.to_string(),
        }),
    }
}

/// Delete a book for a student
#[utoipa::path(
    delete,
    path = "/api/students/{student_id}/books/{book_id}",
    params(
        ("student_id" = i64, Path, description = "Student ID"),
        ("book_id" = i64, Path, description = "Book ID"),
    ),
    responses(
        (status = 200, description = "Student book deleted successfully"),
        (status = 500, description = "Internal server error", body = ErrorResponse),
    )
)]
#[delete("/students/{student_id}/books/{book_id}")]
pub async fn delete_student_book_endpoint(
    db: web::Data<SqlitePool>,
    path: web::Path<(i64, i64)>,
) -> impl Responder {
    let (student_id, book_id) = path.into_inner();
    match delete_student_book(&db, student_id, book_id).await {
        Ok(_) => HttpResponse::Ok().json("Student book deleted successfully"),
        Err(e) => HttpResponse::InternalServerError().json(ErrorResponse {
            error: e.to_string(),
        }),
    }
}

/// Start a learning session
#[utoipa::path(
    post,
    path = "/api/students/{student_id}/books/{book_id}/learn",
    params(
        ("student_id" = i64, Path, description = "Student ID"),
        ("book_id" = i64, Path, description = "Book ID"),
    ),
    responses(
        (status = 200, description = "Learning session started"),
        (status = 500, description = "Internal server error", body = ErrorResponse),
    )
)]
#[post("/students/{student_id}/books/{book_id}/learn")]
pub async fn start_learning(
    library: web::Data<Arc<Library>>,
    db: web::Data<SqlitePool>,
    path: web::Path<(i64, i64)>,
) -> impl Responder {
    let (student_id, book_id) = path.into_inner();
    match TeacherAgent::new(
        library.get_ref().clone(),
        student_id,
        book_id,
        db.get_ref().clone(),
    )
    .await
    {
        Ok(teacher) => {
            // TODO: Implement WebSocket connection for real-time learning
            HttpResponse::Ok().json("Learning session started")
        }
        Err(e) => HttpResponse::InternalServerError().json(ErrorResponse {
            error: e.to_string(),
        }),
    }
}
