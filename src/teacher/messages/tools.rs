use crate::ai_utils::Tool;

use super::{
    MessagesDatabase,
    progress::{BookProgress, ChapterProgress},
};

pub struct ProgressUpdateTool {
    messages_db: MessagesDatabase,
}

impl ProgressUpdateTool {
    pub fn new(messages_db: MessagesDatabase) -> Self {
        Self { messages_db }
    }
}

impl Tool for ProgressUpdateTool {
    type Args = ChapterProgress;
    type Output = ChapterProgress;
    fn name(&self) -> String {
        "ProgressUpdate".to_string()
    }
    fn description(&self) -> Option<String> {
        Some("Update the progress of a chapter".to_string())
    }
    async fn call(&self, args: Self::Args) -> anyhow::Result<Self::Output> {
        self.messages_db.update_chapter_progress(args).await
    }
}

pub struct AddMemoryTool {
    messages_db: MessagesDatabase,
}

impl AddMemoryTool {
    pub fn new(messages_db: MessagesDatabase) -> Self {
        Self { messages_db }
    }
}

impl Tool for AddMemoryTool {
    type Args = String;
    type Output = ();
    fn name(&self) -> String {
        "AddMemory".to_string()
    }
    fn description(&self) -> Option<String> {
        Some("Add a memory to the book progress".to_string())
    }
    async fn call(&self, args: Self::Args) -> anyhow::Result<Self::Output> {
        self.messages_db.add_memory(args).await
    }
}

pub struct GetBookProgressTool {
    messages_db: MessagesDatabase,
}

impl GetBookProgressTool {
    pub fn new(messages_db: MessagesDatabase) -> Self {
        Self { messages_db }
    }
}

impl Tool for GetBookProgressTool {
    type Args = ();
    type Output = BookProgress;
    fn name(&self) -> String {
        "GetBookProgress".to_string()
    }
    fn description(&self) -> Option<String> {
        Some("Get the progress of the book".to_string())
    }
    async fn call(&self, _args: Self::Args) -> anyhow::Result<Self::Output> {
        self.messages_db.get_book_progress().await
    }
}
