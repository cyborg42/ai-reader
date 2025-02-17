use poem_openapi::payload::Json;

use super::OpenApiResult;


pub(crate) async fn get_menu() -> OpenApiResult<String> {
    OpenApiResult::Success(Json("hello".to_string()))
}