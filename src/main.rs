use std::path::PathBuf;

use book_server::{functions::Api, utils::init_log};
use clap::Parser;
use poem::{listener::TcpListener, Route, Server};
use poem_openapi::OpenApiService;
use tracing::info;

/// 默认服务地址
const DEFAULT_SERVER_ADDR: &str = "0.0.0.0:3000";
#[derive(Debug, Parser)]
struct Args {
    /// 日志文件
    #[arg(short, long)]
    log: Option<PathBuf>,
    /// 服务地址
    #[arg(short, long, default_value = DEFAULT_SERVER_ADDR)]
    server_addr: String,
    /// 书籍路径
    #[arg(short, long)]
    book_path: PathBuf,
}

/// 运行 Poem 服务器
#[tokio::main]
async fn main() -> Result<(), std::io::Error> {
    let args = Args::parse();
    println!("Server address: {}", args.server_addr);
    let _guard = init_log(args.log);

    info!("Server address: {}", args.server_addr);
    // 创建 API 服务
    let api_service = OpenApiService::new(Api, "function call", "1.0").server("/api");

    let swagger_ui = api_service.swagger_ui();

    // 生成 openapi.json
    let openapi_json = api_service.spec_endpoint();

    // 定义路由
    let app = Route::new()
        .nest("/api", api_service)
        .nest("/openapi.json", openapi_json) //  curl -X get http://localhost:3000/openapi.json
        .nest("/swagger", swagger_ui); // http://localhost:3000/swagger

    // 启动服务器
    Server::new(TcpListener::bind(args.server_addr))
        .run(app)
        .await
}
