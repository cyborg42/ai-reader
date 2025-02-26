use std::path::PathBuf;

use book_server::utils::init_log;
use clap::Parser;
use sqlx::SqlitePool;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    /// Path to database file
    #[arg(short, long, default_value = "./database/book.db")]
    database: PathBuf,

    /// Path to book base directory
    #[arg(short, long, default_value = "./test-book/Rust for Rustaceans/src")]
    bookbase: PathBuf,

    /// Path to OpenAI API key file
    #[arg(short, long, default_value = "./openai_api_key.toml")]
    openai_key: PathBuf,
}

/// 运行 Poem 服务器
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let _guard = init_log(None);
    let args = Cli::parse();

    let database = SqlitePool::connect(&args.database.to_string_lossy()).await?;

    // Ensure foreign keys are enabled
    sqlx::query("PRAGMA foreign_keys = ON")
        .execute(&database)
        .await?;

    Ok(())
}

// #[tokio::test]
// async fn test_library() {
//     let database = SqlitePool::connect("./database/book.db").await.unwrap();
//     // sqlx::query!(
//     //     "insert into test_time default values",
//     // ).execute(&database).await.unwrap();
//     let now = now_local();
//     println!("{:?}", now);
//     sqlx::query!("insert into test_time (ti) values (?)", now)
//         .execute(&database)
//         .await
//         .unwrap();
//     let t = sqlx::query_scalar!("select ti from test_time where id = 1",)
//         .fetch_one(&database)
//         .await
//         .unwrap();
//     println!("{:?}", t.to_offset(*LOCAL_OFFSET));
// }
