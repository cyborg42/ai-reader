use std::{convert::Infallible, sync::Arc, time::Duration};

use async_openai::types::{
    ChatCompletionRequestAssistantMessageContent, ChatCompletionRequestAssistantMessageContentPart,
    ChatCompletionRequestMessage, ChatCompletionRequestToolMessageContent,
    ChatCompletionRequestToolMessageContentPart, ChatCompletionRequestUserMessageContent,
    ChatCompletionRequestUserMessageContentPart,
};
use axum::{
    Extension, Router,
    extract::{Json, Multipart, Query, State},
    response::{
        IntoResponse, Sse,
        sse::{self, Event},
    },
    routing::{get, post},
};
use moka::future::Cache;
use serde::{Deserialize, Serialize};
use tokio::sync::{Mutex, mpsc::channel};
use tokio_stream::wrappers::ReceiverStream;
use tower_sessions::Session;
use utoipa::ToSchema;

use crate::{
    books::{book::BookMeta, library::Library},
    student::{self, StudentInfo},
    teacher::TeacherAgent,
};

use super::upload_books;

#[derive(Deserialize, ToSchema)]
pub struct CreateUserRequest {
    pub name: String,
    pub email: String,
    pub password: String,
}

#[utoipa::path(
    context_path = "/api/user",
    path = "/create_user",
    method(post),
    request_body = CreateUserRequest,
    responses(
        (status = 200, description = "User created successfully"),
        (status = 400, description = "Bad request")
    )
)]
pub async fn create_user(
    State(library): State<Arc<Library>>,
    Json(req): Json<CreateUserRequest>,
) -> impl IntoResponse {
    let db = library.database.clone();
    match student::create_student(&db, req.name, req.email, req.password).await {
        Ok(_) => "User created successfully".into_response(),
        Err(e) => (axum::http::StatusCode::BAD_REQUEST, e.to_string()).into_response(),
    }
}

#[derive(Deserialize, ToSchema)]
pub struct LoginRequest {
    pub email: String,
    pub password: String,
}

#[utoipa::path(
    context_path = "/api/user",
    path = "/login",
    method(post),
    request_body = LoginRequest,
    responses(
        (status = 200, description = "Login successful"),
        (status = 400, description = "Invalid credentials")
    )
)]
pub async fn login(
    State(library): State<Arc<Library>>,
    session: Session,
    Json(req): Json<LoginRequest>,
) -> impl IntoResponse {
    let db = library.database.clone();
    let email = req.email;
    let password = req.password;
    match student::login(&db, email, password).await {
        Ok(id) => {
            session.insert("student_id", id).await.unwrap();
            "Login successful".into_response()
        }
        Err(e) => (axum::http::StatusCode::BAD_REQUEST, e.to_string()).into_response(),
    }
}

#[utoipa::path(
    context_path = "/api/user",
    path = "/user_info",
    method(get),
    responses(
        (status = 200, description = "User info", body = StudentInfo),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn user_info(State(library): State<Arc<Library>>, session: Session) -> impl IntoResponse {
    let db = library.database.clone();
    let Ok(Some(student_id)) = session.get::<i64>("student_id").await else {
        return (axum::http::StatusCode::UNAUTHORIZED, ()).into_response();
    };
    match student::get_student_info(&db, student_id).await {
        Ok(user) => Json(user).into_response(),
        Err(e) => (axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

#[utoipa::path(
    context_path = "/api/user",
    path = "/logout",
    method(post),
    responses(
        (status = 200, description = "Logout successful")
    )
)]
pub async fn logout(session: Session) -> impl IntoResponse {
    let _ = session.delete().await;
    "Logout successful".into_response()
}

#[utoipa::path(
    context_path = "/api/user",
    path = "/list_books",
    method(get),
    responses(
        (status = 200, description = "List of books", body = Vec<BookMeta>),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn list_books(
    State(library): State<Arc<Library>>,
    session: Session,
) -> impl IntoResponse {
    let db = library.database.clone();
    let Ok(Some(student_id)) = session.get::<i64>("student_id").await else {
        return (axum::http::StatusCode::UNAUTHORIZED, ()).into_response();
    };
    match student::get_student_books(&db, student_id).await {
        Ok(books) => Json(books).into_response(),
        Err(e) => (axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

#[utoipa::path(
    context_path = "/api/user",
    path = "/upload_and_add_books",
    method(post),
    responses(
        (status = 200, description = "Upload successful"),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn upload_and_add_books(
    State(library): State<Arc<Library>>,
    session: Session,
    multipart: Multipart,
) -> impl IntoResponse {
    let db = library.database.clone();
    let Ok(Some(student_id)) = session.get::<i64>("student_id").await else {
        return (axum::http::StatusCode::UNAUTHORIZED, ()).into_response();
    };
    match upload_books(multipart, library).await {
        Ok(book_ids) => match student::add_student_books(&db, student_id, book_ids).await {
            Ok(_) => "Upload successful".into_response(),
            Err(e) => {
                (axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response()
            }
        },
        Err(e) => (axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

#[utoipa::path(
    context_path = "/api/user",
    path = "/add_book",
    method(post),
    params(
        ("book_id" = i64, Query, description = "ID of the book to add")
    ),
    responses(
        (status = 200, description = "Book added successfully"),
        (status = 401, description = "Unauthorized"),
        (status = 400, description = "Bad request")
    )
)]
pub async fn add_book(
    State(library): State<Arc<Library>>,
    session: Session,
    Query(book_id): Query<i64>,
) -> impl IntoResponse {
    let db = library.database.clone();
    let Ok(Some(student_id)) = session.get::<i64>("student_id").await else {
        return (axum::http::StatusCode::UNAUTHORIZED, ()).into_response();
    };
    match TeacherAgent::init(student_id, book_id, db).await {
        Ok(_) => ().into_response(),
        Err(e) => (axum::http::StatusCode::BAD_REQUEST, e.to_string()).into_response(),
    }
}

#[utoipa::path(
    context_path = "/api/user",
    path = "/delete_book",
    method(post),
    params(
        ("book_id" = i64, Query, description = "ID of the book to delete")
    ),
    responses(
        (status = 200, description = "Book deleted successfully"),
        (status = 401, description = "Unauthorized"),
        (status = 400, description = "Bad request")
    )
)]
pub async fn delete_book(
    State(library): State<Arc<Library>>,
    session: Session,
    Query(book_id): Query<i64>,
) -> impl IntoResponse {
    let db = library.database.clone();
    let Ok(Some(student_id)) = session.get::<i64>("student_id").await else {
        return (axum::http::StatusCode::UNAUTHORIZED, ()).into_response();
    };
    match student::delete_student_book(&db, student_id, book_id).await {
        Ok(_) => ().into_response(),
        Err(e) => (axum::http::StatusCode::BAD_REQUEST, e.to_string()).into_response(),
    }
}

type TeacherAgentCache = Cache<(i64, i64), Arc<Mutex<TeacherAgent>>>;

#[derive(Serialize, ToSchema)]
pub enum ConversationMessage {
    User {
        content: String,
    },
    Assistant {
        content: String,
        tool_calls: Vec<String>,
    },
    Tool {
        content: String,
    },
}

impl TryFrom<ChatCompletionRequestMessage> for ConversationMessage {
    type Error = ();

    fn try_from(message: ChatCompletionRequestMessage) -> Result<Self, ()> {
        match message {
            ChatCompletionRequestMessage::User(msg) => match msg.content {
                ChatCompletionRequestUserMessageContent::Text(text) => {
                    Ok(Self::User { content: text })
                }
                ChatCompletionRequestUserMessageContent::Array(arr) => {
                    let mut content = String::new();
                    for a in arr {
                        let ChatCompletionRequestUserMessageContentPart::Text(text) = a else {
                            continue;
                        };
                        content.push_str(&text.text);
                    }
                    Ok(Self::User { content })
                }
            },
            ChatCompletionRequestMessage::Assistant(msg) => {
                let mut content = String::new();
                match msg.content {
                    Some(ChatCompletionRequestAssistantMessageContent::Text(t)) => content = t,
                    Some(ChatCompletionRequestAssistantMessageContent::Array(arr)) => {
                        for a in arr {
                            let ChatCompletionRequestAssistantMessageContentPart::Text(t) = a
                            else {
                                continue;
                            };
                            content.push_str(&t.text);
                        }
                    }
                    None => {}
                }
                let tool_calls = msg
                    .tool_calls
                    .unwrap_or_default()
                    .into_iter()
                    .map(|t| t.function.name)
                    .collect();
                Ok(Self::Assistant {
                    content,
                    tool_calls,
                })
            }
            ChatCompletionRequestMessage::Tool(msg) => {
                let mut content = String::new();
                match msg.content {
                    ChatCompletionRequestToolMessageContent::Text(t) => content = t,
                    ChatCompletionRequestToolMessageContent::Array(arr) => {
                        for a in arr {
                            let ChatCompletionRequestToolMessageContentPart::Text(t) = a;
                            content.push_str(&t.text);
                        }
                    }
                };
                Ok(Self::Tool { content })
            }
            _ => Err(()),
        }
    }
}

#[utoipa::path(
    context_path = "/api/user",
    path = "/get_conversation",
    method(get),
    params(
        ("book_id" = i64, Query, description = "ID of the book to get conversation for")
    ),
    responses(
        (status = 200, description = "Conversation", body = Vec<ConversationMessage>),
    )
)]
pub async fn get_conversation(
    State(library): State<Arc<Library>>,
    Extension(cache): Extension<Arc<TeacherAgentCache>>,
    session: Session,
    Query(book_id): Query<i64>,
) -> impl IntoResponse {
    let Ok(Some(student_id)) = session.get::<i64>("student_id").await else {
        return (axum::http::StatusCode::UNAUTHORIZED, ()).into_response();
    };
    let teacher = match cache
        .try_get_with((student_id, book_id), async move {
            match TeacherAgent::new(library, student_id, book_id).await {
                Ok(teacher) => {
                    let teacher = Arc::new(Mutex::new(teacher));
                    Ok(teacher)
                }
                Err(e) => Err(e.to_string()),
            }
        })
        .await
    {
        Ok(teacher) => teacher,
        Err(e) => {
            return (axum::http::StatusCode::BAD_REQUEST, e.to_string()).into_response();
        }
    };
    let teacher = teacher.lock().await;
    let history: Vec<ConversationMessage> = teacher
        .get_conversation()
        .await
        .into_iter()
        .filter_map(|m| ConversationMessage::try_from(m).ok())
        .collect();
    Json(history).into_response()
}

#[derive(Deserialize, ToSchema)]
pub struct ChatRequest {
    book_id: i64,
    message: String,
}

#[utoipa::path(
    context_path = "/api/user",
    path = "/chat",
    method(post),
    request_body = ChatRequest,
    responses(
        (status = 200, description = "Chat response stream", content_type = "text/event-stream"),
        (status = 401, description = "Unauthorized"),
        (status = 400, description = "Bad request")
    )
)]
pub async fn chat(
    State(library): State<Arc<Library>>,
    Extension(cache): Extension<Arc<TeacherAgentCache>>,
    session: Session,
    Json(req): Json<ChatRequest>,
) -> impl IntoResponse {
    let Ok(Some(student_id)) = session.get::<i64>("student_id").await else {
        return (axum::http::StatusCode::UNAUTHORIZED, ()).into_response();
    };
    let ChatRequest { book_id, message } = req;
    let teacher = match cache
        .try_get_with((student_id, book_id), async move {
            match TeacherAgent::new(library, student_id, book_id).await {
                Ok(teacher) => {
                    let teacher = Arc::new(Mutex::new(teacher));
                    Ok(teacher)
                }
                Err(e) => Err(e.to_string()),
            }
        })
        .await
    {
        Ok(teacher) => teacher,
        Err(e) => {
            return (axum::http::StatusCode::BAD_REQUEST, e.to_string()).into_response();
        }
    };
    let (tx, rx) = channel::<Result<Event, Infallible>>(100);
    tokio::spawn(async move {
        let mut teacher = teacher.lock().await;
        let _ = teacher.input(message.into(), tx).await;
    });

    let stream = ReceiverStream::new(rx);
    let sse = Sse::new(stream).keep_alive(sse::KeepAlive::new().interval(Duration::from_secs(10)));

    sse.into_response()
}

pub fn get_user_scope(cache: Arc<TeacherAgentCache>) -> Router<Arc<Library>> {
    Router::new().nest(
        "/user",
        Router::new()
            .route("/create_user", post(create_user))
            .route("/login", post(login))
            .route("/user_info", get(user_info))
            .route("/logout", post(logout))
            .route("/list_books", get(list_books))
            .route("/delete_book", post(delete_book))
            .route("/add_book", post(add_book))
            .route("/upload_and_add_books", post(upload_and_add_books))
            .route(
                "/get_conversation",
                get(get_conversation).layer(Extension(cache.clone())),
            )
            .route("/chat", post(chat).layer(Extension(cache))),
    )
}
