use book_server::functions::get_openapi_json;

fn main() {
    let path = std::env::args()
        .nth(1)
        .unwrap_or("openapi.json".to_string());
    let json =
        get_openapi_json().replace(r"application/json; charset=utf-8", r"application/json");
    std::fs::write(path, json).unwrap();
}
