use std::sync::Arc;
use std::{net::SocketAddr, path::PathBuf};

use axum::Router;
use axum_server::tls_rustls::RustlsConfig;
use ai_reader::{
    api::{manager::get_manager_scope, public::get_public_scope, user::get_user_scope},
    books::library::Library,
    utils::init_log,
};
use clap::Parser;
use moka::future::Cache;
use sqlx::SqlitePool;
use time::Duration;
use tower_http::{cors::CorsLayer, trace::TraceLayer};
use tower_sessions::{CachingSessionStore, Expiry, SessionManagerLayer};
use tower_sessions_moka_store::MokaStore;
use tower_sessions_sqlx_store::SqliteStore;
use tracing::info;
use utoipa::OpenApi;
use utoipa_swagger_ui::SwaggerUi;

#[derive(Debug, Parser)]
#[command(disable_help_flag = true)]
struct Args {
    #[arg(short, long, default_value = "database/book.db")]
    database: PathBuf,
    #[arg(short, long, default_value = "bookbase")]
    bookbase: PathBuf,
    #[arg(short = 'H', long, default_value = "127.0.0.1")]
    host: String,
    #[arg(short, long, default_value = "8080")]
    port: u16,
    #[arg(short, long, default_value = "database/session.db")]
    session_database: PathBuf,
}

#[derive(OpenApi)]
#[openapi(paths(
    ai_reader::api::user::create_user,
    ai_reader::api::user::login,
    ai_reader::api::user::logout,
    ai_reader::api::user::user_info,
    ai_reader::api::user::list_books,
    ai_reader::api::user::upload_and_add_books,
    ai_reader::api::user::add_book,
    ai_reader::api::user::delete_book,
    ai_reader::api::user::get_conversation,
    ai_reader::api::user::chat,
    ai_reader::api::public::get_public_books,
))]
struct UserApiDoc;

#[derive(OpenApi)]
#[openapi(paths(
    ai_reader::api::manager::login,
    ai_reader::api::manager::logout,
    ai_reader::api::manager::list_books,
    ai_reader::api::manager::upload_public_book,
    ai_reader::api::manager::remove_book,
    ai_reader::api::manager::set_book_public,
    ai_reader::api::manager::list_students,
    ai_reader::api::public::get_public_books,
))]
struct ManagerApiDoc;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let _guard = init_log(None);
    let args = Args::parse();

    // Initialize crypto provider for Rustls
    rustls::crypto::ring::default_provider()
        .install_default()
        .expect("Failed to install default crypto provider");

    let database = SqlitePool::connect(&args.database.to_string_lossy()).await?;
    let library = Arc::new(Library::new(database.clone(), args.bookbase).await?);

    let sqlite_store = init_session_database(args.session_database).await?;
    let moka_store = MokaStore::new(Some(2000));
    let caching_store = CachingSessionStore::new(moka_store, sqlite_store);
    let session_layer = SessionManagerLayer::new(caching_store)
        .with_expiry(Expiry::OnInactivity(Duration::days(5)));

    // Initialize teacher cache
    let cache = Arc::new(Cache::new(1000));

    // Build the router
    let app = Router::new()
        .merge(
            SwaggerUi::new("/swagger-ui")
                .url("/api-docs/user/openapi.json", UserApiDoc::openapi())
                .url("/api-docs/manager/openapi.json", ManagerApiDoc::openapi()),
        )
        .nest(
            "/api",
            Router::new()
                .merge(get_user_scope(cache.clone()))
                .merge(get_manager_scope())
                .merge(get_public_scope()),
        )
        .with_state(library)
        .layer(session_layer)
        .layer(TraceLayer::new_for_http())
        .layer(CorsLayer::permissive());

    // Start the server
    let listener = SocketAddr::new(args.host.parse()?, args.port);
    let tls_config = RustlsConfig::from_pem_file("./cert.pem", "./key.pem").await?;

    info!("Starting server at {}", listener);
    info!(
        "Swagger UI available at https://{}:{}/swagger-ui/",
        args.host, args.port
    );

    axum_server::bind_rustls(listener, tls_config)
        .serve(app.into_make_service())
        .await?;
    Ok(())
}

async fn init_session_database(path: PathBuf) -> anyhow::Result<SqliteStore> {
    if !path.exists() {
        // Create parent directories if they don't exist
        if let Some(parent) = path.parent() {
            if !parent.exists() {
                tokio::fs::create_dir_all(parent).await?;
            }
        }
        // Create an empty file
        let _ = tokio::fs::File::create(&path).await?;
    }
    let pool =
        tower_sessions_sqlx_store::sqlx::SqlitePool::connect(&path.to_string_lossy()).await?;
    let store = SqliteStore::new(pool);
    store.migrate().await?;
    Ok(store)
}
