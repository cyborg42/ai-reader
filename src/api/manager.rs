use actix_multipart::Multipart;
use actix_session::Session;
use actix_web::{
    HttpResponse, Responder, Scope,
    dev::{ServiceFactory, ServiceRequest, ServiceResponse},
    get, post, web,
};
use argon2::{Argon2, PasswordHash, PasswordVerifier};
use serde::Deserialize;
use sqlx::SqlitePool;
use utoipa::ToSchema;

use crate::{book::library::Library, student};

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

#[utoipa::path(context_path = "/api/manager")]
#[post("/login")]
pub async fn login(
    session: Session,
    req: web::Json<LoginRequest>,
    db: web::Data<SqlitePool>,
) -> impl Responder {
    let LoginRequest { email, password } = req.into_inner();
    match manager_login(db.as_ref(), email, password).await {
        Ok(id) => {
            session.insert("manager_id", id).unwrap();
            HttpResponse::Ok().body("Login successful")
        }
        Err(e) => HttpResponse::BadRequest().body(e.to_string()),
    }
}

#[utoipa::path(context_path = "/api/manager")]
#[post("/logout")]
pub async fn logout(session: Session) -> impl Responder {
    session.purge();
    HttpResponse::Ok().body("Logout successful")
}

#[utoipa::path(context_path = "/api/manager")]
#[get("/list_books")]
pub async fn list_books(session: Session, library: web::Data<Library>) -> impl Responder {
    let Ok(Some(_)) = session.get::<i64>("manager_id") else {
        return HttpResponse::Unauthorized().body(());
    };
    match library.get_book_list(false).await {
        Ok(books) => HttpResponse::Ok().json(books),
        Err(e) => HttpResponse::InternalServerError().body(e.to_string()),
    }
}

#[utoipa::path(context_path = "/api/manager")]
#[post("/upload_public_book")]
pub async fn upload_public_book(
    session: Session,
    payload: Multipart,
    library: web::Data<Library>,
) -> impl Responder {
    let Ok(Some(_)) = session.get::<i64>("manager_id") else {
        return HttpResponse::Unauthorized().body(());
    };
    match upload_books(payload, library.into_inner()).await {
        Ok(book_ids) => HttpResponse::Ok().json(book_ids),
        Err(e) => HttpResponse::InternalServerError().body(e.to_string()),
    }
}

#[utoipa::path(context_path = "/api/manager")]
#[post("/remove_book")]
pub async fn remove_book(
    session: Session,
    book_id: web::Query<i64>,
    library: web::Data<Library>,
) -> impl Responder {
    let Ok(Some(_)) = session.get::<i64>("manager_id") else {
        return HttpResponse::Unauthorized().body(());
    };
    match library.delete_book(book_id.into_inner()).await {
        Ok(_) => HttpResponse::Ok().body("Book removed successfully"),
        Err(e) => HttpResponse::InternalServerError().body(e.to_string()),
    }
}

#[utoipa::path(context_path = "/api/manager")]
#[post("/set_book_public")]
pub async fn set_book_public(
    session: Session,
    book_id: web::Query<i64>,
    is_public: web::Query<bool>,
    library: web::Data<Library>,
) -> impl Responder {
    let Ok(Some(_)) = session.get::<i64>("manager_id") else {
        return HttpResponse::Unauthorized().body(());
    };
    match library
        .set_book_public(book_id.into_inner(), is_public.into_inner())
        .await
    {
        Ok(_) => HttpResponse::Ok().body("Book public set successfully"),
        Err(e) => HttpResponse::InternalServerError().body(e.to_string()),
    }
}

#[utoipa::path(context_path = "/api/manager")]
#[get("/list_students")]
pub async fn list_students(session: Session, db: web::Data<SqlitePool>) -> impl Responder {
    let Ok(Some(_)) = session.get::<i64>("manager_id") else {
        return HttpResponse::Unauthorized().body(());
    };
    match student::get_student_list(db.as_ref()).await {
        Ok(students) => HttpResponse::Ok().json(students),
        Err(e) => HttpResponse::InternalServerError().body(e.to_string()),
    }
}

pub fn get_manager_scope() -> Scope<
    impl ServiceFactory<
        ServiceRequest,
        Config = (),
        Response = ServiceResponse,
        Error = actix_web::Error,
        InitError = (),
    >,
> {
    web::scope("/manager")
        .service(login)
        .service(logout)
        .service(list_books)
        .service(upload_public_book)
        .service(remove_book)
        .service(set_book_public)
        .service(list_students)
}
