use std::path::PathBuf;

use book_server::{book::BookServer, llm_fn::set_openai_api_key, utils::init_log};
use clap::Parser;
use poem::{Route, Server, listener::TcpListener};
use poem_openapi::OpenApiService;
use tracing::info;

/// 默认服务地址
const DEFAULT_SERVER_ADDR: &str = "0.0.0.0:3000";
#[derive(Debug, Parser)]
struct Args {
    #[arg(short, long)]
    log: Option<PathBuf>,
    #[arg(short, long, default_value = DEFAULT_SERVER_ADDR)]
    server_addr: String,
    #[arg(short, long, default_value = "./test-book")]
    book_path: PathBuf,
    #[arg(short, long, default_value = "./database")]
    database: PathBuf,
    #[arg(short, long, default_value = "./openai_api_key.toml")]
    openai_api_key_path: PathBuf,
}

/// 运行 Poem 服务器
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    println!("Server address: {}", args.server_addr);
    let _guard = init_log(args.log);
    info!("Server address: {}", args.server_addr);
    set_openai_api_key(args.openai_api_key_path);
    // 创建 API 服务
    let api_service = OpenApiService::new(
        BookServer::new(args.database, args.book_path).await?,
        "function call",
        "1.0",
    )
    .server("/api");

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
        .await?;
    Ok(())
}
