use std::path::PathBuf;

use actix_session::{
    SessionMiddleware,
    config::{PersistentSession, TtlExtensionPolicy},
    storage::RedisSessionStore,
};
use actix_web::{App, HttpServer, cookie::Key, web};
use book_server::{
    api::{manager::get_manager_scope, public::get_public_scope, user::get_user_scope},
    books::library::Library,
    utils::init_log,
};
use clap::Parser;
use moka::future::Cache;
use rustls::crypto::{CryptoProvider, ring};
use rustls::pki_types::{CertificateDer, PrivateKeyDer, pem::PemObject};
use sqlx::SqlitePool;
use time::Duration;
use tracing_actix_web::TracingLogger;
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
    #[arg(short, long, default_value = "redis://127.0.0.1:6379")]
    redis_url: String,
}

#[derive(OpenApi)]
#[openapi(paths(
    book_server::api::user::create_user,
    book_server::api::user::login,
    book_server::api::user::logout,
    book_server::api::user::user_info,
    book_server::api::user::list_books,
    book_server::api::user::upload_and_add_books,
    book_server::api::user::add_book,
    book_server::api::user::delete_book,
    book_server::api::user::chat,
    book_server::api::public::get_public_books,
))]
struct UserApiDoc;

#[derive(OpenApi)]
#[openapi(paths(
    book_server::api::manager::login,
    book_server::api::manager::logout,
    book_server::api::manager::list_books,
    book_server::api::manager::upload_public_book,
    book_server::api::manager::remove_book,
    book_server::api::manager::set_book_public,
    book_server::api::manager::list_students,
    book_server::api::public::get_public_books,
))]
struct ManagerApiDoc;

#[actix_web::main]
async fn main() -> anyhow::Result<()> {
    let _guard = init_log(None);
    let args = Args::parse();

    // Initialize rustls crypto provider
    CryptoProvider::install_default(ring::default_provider())
        .map_err(|e| anyhow::anyhow!("Failed to initialize rustls crypto provider: {:?}", e))?;

    println!("Starting server at https://{}:{}", args.host, args.port);
    println!(
        "Swagger UI available at https://{}:{}/swagger-ui/",
        args.host, args.port
    );
    let database = SqlitePool::connect(&args.database.to_string_lossy()).await?;
    let library = web::Data::new(Library::new(database.clone(), args.bookbase).await?);
    let database = web::Data::new(database.clone());
    let certs = CertificateDer::pem_file_iter("cert.pem")
        .unwrap()
        .map(|c| c.unwrap())
        .collect();
    let private_key = PrivateKeyDer::from_pem_file("key.pem").unwrap();
    let config = rustls::ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(certs, private_key)
        .unwrap();

    let store = RedisSessionStore::new(&args.redis_url).await?;
    let key = dotenvy::var("SESSION_SECRET").unwrap();
    let key = Key::from(key.as_bytes());
    let cache = web::Data::new(Cache::new(1000));
    HttpServer::new(move || {
        let session_middleware = SessionMiddleware::builder(store.clone(), key.clone())
            .session_lifecycle(
                PersistentSession::default()
                    .session_ttl(Duration::days(5))
                    .session_ttl_extension_policy(TtlExtensionPolicy::OnEveryRequest),
            )
            .build();
        App::new()
            .wrap(TracingLogger::default())
            .service(
                SwaggerUi::new("/swagger-ui/{_:.*}")
                    .url("/api-docs/user/openapi.json", UserApiDoc::openapi())
                    .url("/api-docs/manager/openapi.json", ManagerApiDoc::openapi()),
            )
            .service(
                web::scope("/api")
                    .wrap(session_middleware)
                    .app_data(library.clone())
                    .app_data(database.clone())
                    .service(get_user_scope(cache.clone()))
                    .service(get_manager_scope())
                    .service(get_public_scope()),
            )
    })
    .bind_rustls_0_23((args.host, args.port), config)?
    .run()
    .await?;

    Ok(())
}
