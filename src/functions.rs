mod book_info;

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

impl<T: ToJSON, E: std::fmt::Display> From<Result<T, E>> for OpenApiResult<T> {
    fn from(value: Result<T, E>) -> Self {
        match value {
            Ok(value) => OpenApiResult::Success(Json(value)),
            Err(e) => OpenApiResult::Error(Json(e.to_string())),
        }
    }
}
/// 定义 API 端点
#[OpenApi]
impl Api {
    #[oai(path = "/book_info/menu", method = "get")]
    async fn get_menu(&self) -> OpenApiResult<String> {
        book_info::get_menu().await.into()
    }
}

pub fn get_openapi_json() -> String {
    poem_openapi::OpenApiService::new(Api, "book function server", "1.0")
        .server("http://localhost:3000/api")
        .spec()
}
