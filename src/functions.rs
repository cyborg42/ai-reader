use std::path::Path;

use poem_openapi::{payload::Json, OpenApi};

use crate::book::Book;

#[derive(Debug, Clone, Default)]
pub struct BookServer {
    book: Book,
}

impl BookServer {
    pub fn new(src_dir: impl AsRef<Path>) -> Self {
        let book = Book::load(src_dir, 20).unwrap();
        Self { book }
    }
    pub fn menu(&self) -> String {
        format!("{:?}", self.book.title)
    }
}

#[OpenApi]
impl BookServer {
    /// get book menu
    #[oai(path = "/book_info/menu", method = "get")]
    async fn get_menu(&self) -> Json<String> {
        Json(self.menu())
    }
}

pub fn get_openapi_json() -> String {
    poem_openapi::OpenApiService::new(BookServer::default(), "book function server", "1.0")
        .server("http://localhost:3000/api")
        .spec()
}
