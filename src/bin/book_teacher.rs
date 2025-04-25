use std::{path::PathBuf, sync::Arc};

use async_openai::types::ChatCompletionRequestUserMessage;
use book_server::{
    books::library::Library,
    student::{
        create_student, delete_student, delete_student_book, get_student_books, get_student_list,
    },
    teacher::{ResponseEvent, TeacherAgent},
    utils::init_log,
};
use clap::Parser;
use sqlx::SqlitePool;
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    sync::mpsc,
};

#[derive(Debug, clap::Parser)]
struct Args {
    #[command(subcommand)]
    command: Commands,
    #[arg(short, long, default_value = "database/book.db")]
    database: PathBuf,
    #[arg(short, long, default_value = "bookbase")]
    bookbase: PathBuf,
}

#[derive(Debug, clap::Subcommand)]
enum Commands {
    Book {
        #[command(subcommand)]
        command: BookCommand,
    },
    User {
        #[command(subcommand)]
        command: UserCommand,
    },
    Login {
        id: i64,
        #[command(subcommand)]
        command: LoginCommand,
    },
}

#[derive(Debug, clap::Subcommand)]
enum BookCommand {
    List,
    Upload { file: PathBuf },
    UploadDir { dir: PathBuf },
    Delete { id: i64 },
}

#[derive(Debug, clap::Subcommand)]
enum UserCommand {
    List,
    Create {
        #[arg(short, long)]
        name: String,
        #[arg(short, long)]
        email: String,
        #[arg(short, long, default_value = "42")]
        password: String,
    },
    Delete {
        id: i64,
    },
}

#[derive(Debug, clap::Subcommand)]
enum LoginCommand {
    ListBooks,
    Learn { book_id: i64 },
    Delete { book_id: i64 },
}

#[tokio::main]
async fn main() {
    let _guard = init_log(None);
    let args = Args::parse();
    if let Err(e) = run(args).await {
        eprintln!("{:?}", e);
    }
}
async fn run(args: Args) -> anyhow::Result<()> {
    let database = SqlitePool::connect(&args.database.to_string_lossy()).await?;
    let library = Library::new(database.clone(), args.bookbase).await?;

    match args.command {
        Commands::Book { command } => match command {
            BookCommand::List => {
                for book in library.get_book_list(false).await? {
                    println!("{:<20} {}", book.id, book.title);
                }
            }
            BookCommand::Upload { file } => {
                println!("Uploading book from file: {}", file.display());
                library.upload_book(file).await?;
            }
            BookCommand::UploadDir { dir } => {
                println!("Uploading books from directory: {}", dir.display());
                library.upload_books_in_dir(dir).await?;
            }
            BookCommand::Delete { id } => {
                println!("Deleting book with id: {}", id);
                library.delete_book(id).await?;
            }
        },
        Commands::User { command } => match command {
            UserCommand::List => {
                println!("{:#?}", get_student_list(&database).await?);
            }
            UserCommand::Create {
                name,
                email,
                password,
            } => {
                let id = create_student(&database, name, email, password).await?;
                println!("Student created with id: {}", id);
            }
            UserCommand::Delete { id } => {
                delete_student(&database, id).await?;
                println!("Student deleted with id: {}", id);
            }
        },
        Commands::Login { id, command } => match command {
            LoginCommand::Learn { book_id } => {
                TeacherAgent::init(id, book_id, database.clone()).await?;
                let teacher =
                    TeacherAgent::new(Arc::new(library), id, book_id, database.clone()).await?;
                start_learning(teacher).await?;
            }
            LoginCommand::ListBooks => {
                for book in get_student_books(&database, id).await? {
                    println!("{:<20} {}", book.id, book.title);
                }
            }
            LoginCommand::Delete { book_id } => {
                delete_student_book(&database, id, book_id).await?;
                println!("Book deleted with id: {}", book_id);
            }
        },
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CurrentScene {
    Start,
    Content,
    Refusal,
    ToolCall,
    ToolResult,
}

async fn start_learning(mut teacher: TeacherAgent) -> anyhow::Result<()> {
    loop {
        println!("\n[Student]:");
        let stdin = tokio::io::stdin();
        let mut reader = BufReader::new(stdin);
        let mut input = String::new();
        reader.read_line(&mut input).await?;
        let input = input.trim();
        if input == "exit" {
            break;
        }
        let message = ChatCompletionRequestUserMessage::from(input);
        let (tx, mut rx) = mpsc::channel(100);
        let (_, results) = unsafe {
            async_scoped::TokioScope::scope_and_collect(|s| {
                s.spawn(async { teacher.input(message, tx).await });
                s.spawn(async {
                    let mut stdout = tokio::io::stdout();
                    let mut scene = CurrentScene::Start;
                    while let Some(content) = rx.recv().await {
                        match content {
                            ResponseEvent::Content(content) => {
                                if scene != CurrentScene::Content {
                                    stdout.write_all(b"\n[Teacher]:\n").await?;
                                    stdout.flush().await?;
                                    scene = CurrentScene::Content;
                                }
                                stdout.write_all(content.as_bytes()).await?;
                                stdout.flush().await?;
                            }
                            ResponseEvent::Refusal(refusal) => {
                                if scene != CurrentScene::Refusal {
                                    stdout.write_all(b"\n[Refusal]:\n").await?;
                                    stdout.flush().await?;
                                    scene = CurrentScene::Refusal;
                                }
                                stdout.write_all(refusal.to_string().as_bytes()).await?;
                                stdout.flush().await?;
                            }
                            ResponseEvent::ToolCall(call) => {
                                if scene != CurrentScene::ToolCall {
                                    stdout.write_all(b"\n[Tool call]:\n").await?;
                                    stdout.flush().await?;
                                    scene = CurrentScene::ToolCall;
                                }
                                stdout.write_all(format!("{:#?}", call).as_bytes()).await?;
                                stdout.flush().await?;
                            }
                            ResponseEvent::ToolResult(result) => {
                                if scene != CurrentScene::ToolResult {
                                    stdout.write_all(b"\n[Tool result]:\n").await?;
                                    stdout.flush().await?;
                                    scene = CurrentScene::ToolResult;
                                }
                                stdout
                                    .write_all(format!("{:#?}", result).as_bytes())
                                    .await?;
                                stdout.flush().await?;
                            }
                        }
                    }
                    Ok(())
                });
            })
            .await
        };
        for result in results {
            result??;
        }
        println!();
    }
    Ok(())
}
