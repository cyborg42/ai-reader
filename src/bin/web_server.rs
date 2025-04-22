use std::{path::PathBuf, sync::Arc};

use actix_cors::Cors;
use actix_web::{App, HttpServer, web};
use book_server::{
    api::{
        create_student_endpoint, delete_book, delete_student_book_endpoint,
        delete_student_endpoint, list_books, list_student_books, list_students, start_learning,
        upload_book,
    },
    book::{book::Book, library::Library},
    student::StudentInfo,
    utils::init_log,
};
use clap::Parser;
use sqlx::SqlitePool;
use utoipa::OpenApi;

#[derive(Debug, Parser)]
struct Args {
    #[arg(short, long, default_value = "database/book.db")]
    database: PathBuf,
    #[arg(short, long, default_value = "bookbase")]
    bookbase: PathBuf,
    #[arg(short, long, default_value = "127.0.0.1")]
    host: String,
    #[arg(short, long, default_value = "8080")]
    port: u16,
}

#[derive(OpenApi)]
#[openapi(
    paths(
        book_server::api::list_books,
        book_server::api::upload_book,
        book_server::api::delete_book,
        book_server::api::list_students,
        book_server::api::create_student_endpoint,
        book_server::api::delete_student_endpoint,
        book_server::api::list_student_books,
        book_server::api::delete_student_book_endpoint,
        book_server::api::start_learning,
    ),
    components(
        schemas(
            Book,
            StudentInfo,
            book_server::api::UploadBookRequest,
            book_server::api::CreateStudentRequest,
            book_server::api::ErrorResponse,
        )
    ),
    tags(
        (name = "books", description = "Book management endpoints"),
        (name = "students", description = "Student management endpoints"),
        (name = "learning", description = "Learning session endpoints"),
    )
)]
struct ApiDoc;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let _guard = init_log(None);
    let args = Args::parse();

    let database = SqlitePool::connect(&args.database.to_string_lossy()).await?;
    let library = Library::new(database.clone(), args.bookbase).await?;
    let library = Arc::new(library);

    println!("Starting server at http://{}:{}", args.host, args.port);
    println!(
        "Swagger UI available at http://{}:{}/swagger-ui/",
        args.host, args.port
    );

    HttpServer::new(move || {
        let cors = Cors::default()
            .allow_any_origin()
            .allow_any_method()
            .allow_any_header()
            .max_age(3600);

        App::new()
            .wrap(cors)
            .app_data(web::Data::new(database.clone()))
            .app_data(web::Data::new(library.clone()))
            .service(
                web::scope("/api")
                    .service(list_books)
                    .service(upload_book)
                    .service(delete_book)
                    .service(list_students)
                    .service(create_student_endpoint)
                    .service(delete_student_endpoint)
                    .service(list_student_books)
                    .service(delete_student_book_endpoint)
                    .service(start_learning),
            )
    })
    .bind((args.host, args.port))?
    .run()
    .await?;

    Ok(())
}
