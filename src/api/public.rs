use crate::books::book::BookMeta;
use crate::books::library::Library;
use axum::{
    Router,
    extract::{Json, State},
    response::IntoResponse,
    routing::get,
};
use std::sync::Arc;

#[utoipa::path(
    context_path = "/api/public",
    path = "/public_books",
    method(get),
    responses(
        (status = 200, description = "List of public books", body = Vec<BookMeta>),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn get_public_books(State(library): State<Arc<Library>>) -> impl IntoResponse {
    match library.get_book_list(true).await {
        Ok(books) => Json(books).into_response(),
        Err(e) => (axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

pub fn get_public_scope() -> Router<Arc<Library>> {
    Router::new().nest(
        "/public",
        Router::new().route("/public_books", get(get_public_books)),
    )
}
