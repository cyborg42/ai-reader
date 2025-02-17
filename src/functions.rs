mod get_book_info;

use poem_openapi::{payload::Json, types::ToJSON, ApiResponse, OpenApi};

/// API 结构体
pub struct Api;

#[derive(ApiResponse)]
pub enum OpenApiResult<T>
where
    T: ToJSON,
{
    #[oai(status = 200)]
    Success(Json<T>),
    #[oai(status = 400)]
    Error(Json<String>),
}

/// 定义 API 端点
#[OpenApi]
impl Api {
    #[oai(path = "/menu", method = "get")]
    async fn get_menu(&self) -> OpenApiResult<String> {
        get_book_info::get_menu().await
    }
}

pub fn get_openapi_json() -> String {
    poem_openapi::OpenApiService::new(Api, "book function server", "1.0")
        .server("http://localhost:3000/api")
        .spec()
}
