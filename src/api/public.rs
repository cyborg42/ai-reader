use actix_web::{
    HttpResponse, Responder, Scope,
    dev::{ServiceFactory, ServiceRequest, ServiceResponse},
    get, web,
};

use crate::books::library::Library;

#[utoipa::path(context_path = "/api/public")]
#[get("/public_books")]
pub async fn get_public_books(library: web::Data<Library>) -> impl Responder {
    let books = library.get_book_list(true).await;
    match books {
        Ok(books) => HttpResponse::Ok().json(books),
        Err(e) => HttpResponse::InternalServerError().body(e.to_string()),
    }
}

pub fn get_public_scope() -> Scope<
    impl ServiceFactory<
        ServiceRequest,
        Config = (),
        Response = ServiceResponse,
        Error = actix_web::Error,
        InitError = (),
    >,
> {
    web::scope("/public").service(get_public_books)
}
