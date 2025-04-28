pub mod messages;

use std::convert::Infallible;
use std::sync::Arc;

use async_openai::types::{
    ChatCompletionMessageToolCall, ChatCompletionRequestAssistantMessageArgs,
    ChatCompletionRequestToolMessage, ChatCompletionRequestUserMessage,
    CreateChatCompletionRequestArgs,
};
use axum::response::sse::Event;
use futures::StreamExt;
use messages::MessagesManager;
use serde::Serialize;
use sqlx::SqlitePool;
use tokio::sync::mpsc::Sender;

use crate::ai_utils::{AI_CLIENT, AI_MODEL, ToolCallStreamManager, ToolManager};
use crate::books::library::Library;
use crate::books::tools::{BookJumpTool, GetChapterTool};

/// The AI Teacher Agent that interacts with students
pub struct TeacherAgent {
    messages: MessagesManager,
    tool_manager: ToolManager,
}

#[derive(Debug, Clone, Serialize)]
pub enum ResponseEvent {
    Content(String),
    Refusal(String),
    ToolCall(ChatCompletionMessageToolCall),
    ToolResult(ChatCompletionRequestToolMessage),
}

impl TeacherAgent {
    pub async fn init(student_id: i64, book_id: i64, database: SqlitePool) -> anyhow::Result<()> {
        sqlx::query!(
            "insert or ignore into teacher_agent (student_id, book_id, current_chapter_number, memories) values (?, ?, '', '[]')",
            student_id,
            book_id,
        )
        .execute(&database)
        .await?;
        Ok(())
    }
    pub async fn new(
        library: Arc<Library>,
        student_id: i64,
        book_id: i64,
        database: SqlitePool,
    ) -> anyhow::Result<Self> {
        let record = sqlx::query!("select ai_model, token_budget FROM agent_setting")
            .fetch_one(&database)
            .await?;
        let book = library.get_book(book_id).await?;
        let messages =
            MessagesManager::load(student_id, &book, record.token_budget as u64, database).await?;
        let mut tool_manager = ToolManager::default();
        tool_manager.add_tool(GetChapterTool::new(book_id, library.clone()));
        tool_manager.add_tool(BookJumpTool::new(book_id, library.clone()));
        tool_manager.add_tools(messages.get_tools());
        Ok(Self {
            messages,
            tool_manager,
        })
    }
    pub async fn input<E>(
        &mut self,
        msg: ChatCompletionRequestUserMessage,
        tx: Sender<E>,
    ) -> anyhow::Result<()>
    where
        E: From<ResponseEvent> + Send + Sync + 'static,
    {
        self.messages.add_conversation_message(msg).await?;
        let tools = self.tool_manager.get_tools();
        loop {
            let messages = self.messages.get_messages();
            let request = CreateChatCompletionRequestArgs::default()
                .model(AI_MODEL.as_str())
                .messages(messages)
                .tools(tools.clone())
                .build()
                .unwrap();
            let mut stream = AI_CLIENT.chat().create_stream(request).await?;
            let mut tool_call_manager = ToolCallStreamManager::new();
            let mut whole_content = String::new();
            let mut whole_refusal = String::new();
            while let Some(result) = stream.next().await {
                let Some(choice) = result?.choices.pop() else {
                    continue;
                };
                if let Some(content) = choice.delta.content.as_ref() {
                    whole_content.push_str(content);
                    tx.send(ResponseEvent::Content(content.to_string()).into())
                        .await?;
                }
                if let Some(refusal) = choice.delta.refusal.as_ref() {
                    whole_refusal.push_str(refusal);
                }
                if let Some(tool_call_chunks) = choice.delta.tool_calls {
                    tool_call_manager.merge_chunks(tool_call_chunks);
                }
            }
            let mut message_builder = ChatCompletionRequestAssistantMessageArgs::default();
            if !whole_content.is_empty() {
                message_builder.content(whole_content);
            }
            if !whole_refusal.is_empty() {
                tx.send(ResponseEvent::Refusal(whole_refusal.clone()).into())
                    .await?;
                message_builder.refusal(whole_refusal);
            }
            let tool_calls = tool_call_manager.get_tool_calls();
            if !tool_calls.is_empty() {
                message_builder.tool_calls(tool_calls.clone());
            }
            let assistant_message = message_builder.build()?;
            self.messages
                .add_conversation_message(assistant_message)
                .await?;
            if tool_calls.is_empty() {
                break;
            }
            for tool_call in &tool_calls {
                tx.send(ResponseEvent::ToolCall(tool_call.clone()).into())
                    .await?;
            }
            let tool_results = self.tool_manager.call(tool_calls).await;
            for tool_result in &tool_results {
                tx.send(ResponseEvent::ToolResult(tool_result.clone()).into())
                    .await?;
            }
            self.messages
                .add_conversation_messages(tool_results)
                .await?;
        }
        Ok(())
    }
}

impl From<ResponseEvent> for Result<Event, Infallible> {
    fn from(event: ResponseEvent) -> Self {
        Ok(Event::default().json_data(event).unwrap())
    }
}
