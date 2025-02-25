use poem_openapi::OpenApi;
use poem_openapi::payload::Json;

use crate::book::BookServer;

#[OpenApi]
impl BookServer {
    /// get table of contents
    #[oai(path = "/book_info/table_of_contents", method = "get")]
    async fn get_table_of_contents(&self) -> Json<String> {
        Json(format!(""))
    }
}

pub fn get_openapi_json() -> String {
    poem_openapi::OpenApiService::new(BookServer::default(), "book function server", "1.0")
        .server("http://localhost:3000/api")
        .spec()
}
