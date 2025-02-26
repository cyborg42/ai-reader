pub mod messages;

use std::sync::Arc;

use messages::MessagesManager;

use sqlx::SqlitePool;

use crate::book::book::BookInfo;
use crate::book::library::Library;

/// The AI Teacher Agent that interacts with students
pub struct TeacherAgent {
    library: Arc<Library>,
    book_info: BookInfo,
    student_id: i64,
    messages: MessagesManager,
    last_summarize_time: std::time::Instant,
    summarize_interval: std::time::Duration,
    database: SqlitePool,
}
