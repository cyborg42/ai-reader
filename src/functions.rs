mod book_info;
use poem_openapi::{payload::Json, OpenApi};

pub struct Api;

#[OpenApi]
impl Api {
    /// get book menu
    #[oai(path = "/book_info/menu", method = "get")]
    async fn get_menu(&self) -> poem::Result<Json<String>> {
        Ok(Json(book_info::get_menu().await?))
    }
}

pub fn get_openapi_json() -> String {
    poem_openapi::OpenApiService::new(Api, "book function server", "1.0")
        .server("http://localhost:3000/api")
        .spec()
}
