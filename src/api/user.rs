use std::{sync::Arc, time::Duration};

use actix_multipart::Multipart;
use actix_session::Session;
use actix_web::{
    HttpResponse, Responder, Scope,
    dev::{ServiceFactory, ServiceRequest, ServiceResponse},
    get,
    http::header::{CacheControl, CacheDirective, ContentEncoding},
    post, web,
};
use actix_web_lab::sse;
use moka::future::Cache;
use serde::Deserialize;
use sqlx::SqlitePool;
use tokio::sync::{Mutex, mpsc::channel};
use utoipa::ToSchema;

use crate::{books::library::Library, student, teacher::TeacherAgent};

use super::upload_books;

#[derive(Deserialize, ToSchema)]
pub struct CreateUserRequest {
    pub name: String,
    pub email: String,
    pub password: String,
}
#[utoipa::path(context_path = "/api/user")]
#[post("/create_user")]
pub async fn create_user(
    req: web::Json<CreateUserRequest>,
    db: web::Data<SqlitePool>,
) -> impl Responder {
    let req = req.into_inner();
    match student::create_student(&db, req.name, req.email, req.password).await {
        Ok(_) => HttpResponse::Ok().body("User created successfully"),
        Err(e) => HttpResponse::BadRequest().body(e.to_string()),
    }
}

#[derive(Deserialize, ToSchema)]
pub struct LoginRequest {
    pub email: String,
    pub password: String,
}

#[utoipa::path(context_path = "/api/user")]
#[post("/login")]
pub async fn login(
    req: web::Json<LoginRequest>,
    db: web::Data<SqlitePool>,
    session: Session,
) -> impl Responder {
    let req = req.into_inner();
    let email = req.email;
    let password = req.password;
    match student::login(&db, email, password).await {
        Ok(id) => match session.insert("student_id", id) {
            Ok(_) => HttpResponse::Ok().json("Login successful"),
            Err(e) => HttpResponse::InternalServerError().body(e.to_string()),
        },
        Err(e) => HttpResponse::BadRequest().body(e.to_string()),
    }
}

#[utoipa::path(context_path = "/api/user")]
#[get("/user_info")]
pub async fn user_info(session: Session, db: web::Data<SqlitePool>) -> impl Responder {
    let Ok(Some(student_id)) = session.get::<i64>("student_id") else {
        return HttpResponse::Unauthorized().body(());
    };
    match student::get_student_info(&db, student_id).await {
        Ok(user) => HttpResponse::Ok().json(user),
        Err(e) => HttpResponse::InternalServerError().body(e.to_string()),
    }
}

#[utoipa::path(context_path = "/api/user")]
#[post("/logout")]
pub async fn logout(session: Session) -> impl Responder {
    session.purge();
    HttpResponse::Ok().json("Logout successful")
}

#[utoipa::path(context_path = "/api/user")]
#[get("/list_books")]
pub async fn list_books(session: Session, db: web::Data<SqlitePool>) -> impl Responder {
    let Ok(Some(student_id)) = session.get::<i64>("student_id") else {
        return HttpResponse::Unauthorized().body(());
    };
    let books = student::get_student_books(&db, student_id).await;
    match books {
        Ok(books) => HttpResponse::Ok().json(books),
        Err(e) => HttpResponse::InternalServerError().body(e.to_string()),
    }
}

#[utoipa::path(context_path = "/api/user")]
#[post("/upload_and_add_books")]
pub async fn upload_and_add_books(
    session: Session,
    db: web::Data<SqlitePool>,
    library: web::Data<Library>,
    payload: Multipart,
) -> impl Responder {
    let Ok(Some(student_id)) = session.get::<i64>("student_id") else {
        return HttpResponse::Unauthorized().body(());
    };
    let library = library.into_inner();
    match upload_books(payload, library).await {
        Ok(book_ids) => match student::add_student_books(db.as_ref(), student_id, book_ids).await {
            Ok(_) => HttpResponse::Ok().body("Upload successful"),
            Err(e) => HttpResponse::InternalServerError().body(e.to_string()),
        },
        Err(e) => HttpResponse::InternalServerError().body(e.to_string()),
    }
}

#[utoipa::path(context_path = "/api/user")]
#[post("/add_book")]
pub async fn add_book(
    session: Session,
    db: web::Data<SqlitePool>,
    book_id: web::Query<i64>,
) -> impl Responder {
    let Ok(Some(student_id)) = session.get::<i64>("student_id") else {
        return HttpResponse::Unauthorized().body(());
    };
    let book_id = book_id.into_inner();
    match TeacherAgent::init(student_id, book_id, db.as_ref().clone()).await {
        Ok(_) => HttpResponse::Ok().body(()),
        Err(e) => HttpResponse::BadRequest().body(e.to_string()),
    }
}

#[utoipa::path(context_path = "/api/user")]
#[post("/delete_book")]
pub async fn delete_book(
    session: Session,
    db: web::Data<SqlitePool>,
    book_id: web::Query<i64>,
) -> impl Responder {
    let Ok(Some(student_id)) = session.get::<i64>("student_id") else {
        return HttpResponse::Unauthorized().body(());
    };
    let book_id = book_id.into_inner();
    match student::delete_student_book(db.as_ref(), student_id, book_id).await {
        Ok(_) => HttpResponse::Ok().body(()),
        Err(e) => HttpResponse::BadRequest().body(e.to_string()),
    }
}

#[derive(Deserialize, ToSchema)]
pub struct ChatRequest {
    book_id: i64,
    message: String,
}

type TeacherAgentCache = Cache<(i64, i64), Arc<Mutex<TeacherAgent>>>;

#[utoipa::path(context_path = "/api/user")]
#[post("/chat")]
pub async fn chat(
    session: Session,
    db: web::Data<SqlitePool>,
    library: web::Data<Library>,
    cache: web::Data<TeacherAgentCache>,
    req: web::Json<ChatRequest>,
) -> impl Responder {
    let Ok(Some(student_id)) = session.get::<i64>("student_id") else {
        return HttpResponse::Unauthorized().body(());
    };
    let ChatRequest { book_id, message } = req.into_inner();
    let library = library.into_inner();
    let teacher = match cache
        .try_get_with((student_id, book_id), async move {
            match TeacherAgent::new(library, student_id, book_id, db.as_ref().clone()).await {
                Ok(teacher) => {
                    let teacher = Arc::new(Mutex::new(teacher));
                    Ok(teacher)
                }
                Err(e) => Err(e.to_string()),
            }
        })
        .await
    {
        Ok(teacher) => teacher,
        Err(e) => {
            return HttpResponse::BadRequest().body(e.to_string());
        }
    };
    let (tx, rx) = channel(100);
    tokio::spawn(async move {
        let mut teacher = teacher.lock().await;
        let _ = teacher.input(message.into(), tx).await;
    });

    HttpResponse::Ok()
        .content_type(mime::TEXT_EVENT_STREAM)
        .insert_header(ContentEncoding::Identity)
        .insert_header(CacheControl(vec![CacheDirective::NoCache]))
        .body(sse::Sse::from_infallible_receiver(rx).with_retry_duration(Duration::from_secs(10)))
}

pub fn get_user_scope(
    cache: web::Data<TeacherAgentCache>,
) -> Scope<
    impl ServiceFactory<
        ServiceRequest,
        Config = (),
        Response = ServiceResponse,
        Error = actix_web::Error,
        InitError = (),
    >,
> {
    web::scope("/user")
        .app_data(cache)
        .service(create_user)
        .service(login)
        .service(user_info)
        .service(logout)
        .service(list_books)
        .service(chat)
        .service(add_book)
        .service(delete_book)
        .service(upload_and_add_books)
}
