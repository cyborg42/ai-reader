pub mod progress;
pub mod tools;
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

use anyhow::bail;
use async_openai::types::ChatCompletionRequestMessage;
use progress::{BookProgress, ChapterObjective, ChapterProgress, ChapterStatus};
use sqlx::SqlitePool;
use time::OffsetDateTime;
use tools::{AddMemoryTool, GetBookProgressTool, ProgressUpdateTool};

use crate::{
    ai_utils::{Tokens, ToolDyn},
    book::{book::Book, chapter::ChapterNumber},
};

#[derive(Debug, Clone)]
pub struct MessagesDatabase {
    book_id: i64,
    student_id: i64,
    database: SqlitePool,
}

impl MessagesDatabase {
    pub async fn new(book_id: i64, student_id: i64, database: SqlitePool) -> anyhow::Result<Self> {
        sqlx::query!(
            "insert or ignore into teacher_agent (student_id, book_id, current_chapter_number, memories) values (?, ?, '', '[]')",
            student_id,
            book_id,
        )
        .execute(&database)
        .await?;
        Ok(Self {
            book_id,
            student_id,
            database,
        })
    }
    pub async fn get_instruction(&self) -> anyhow::Result<String> {
        let student_name =
            sqlx::query_scalar!("select name from student where id = ?", self.student_id)
                .fetch_one(&self.database)
                .await?;
        let book_name = sqlx::query_scalar!("select title from book where id = ?", self.book_id)
            .fetch_one(&self.database)
            .await?;
        let instruction = format!(
            r#"
## Role:
You are Vera, a sharp-witted AI tutor who loves Agatha Christie, artisanal coffee, linguistics trivia, comic sketching, and noir films. You’re direct, sarcastic yet motivating, expecting {student_name} to keep up while secretly rooting for them.

## Teaching Approach:
- Plan lessons using {book_name}’s structure via [GetChapterContent].
- Deliver chapter-based lessons with clear objectives, engaging activities, and progress tracking.
- Adapt to {student_name}’s needs, balancing critique with encouragement.

## Teaching Process:
1. **Chapter Intro**: Use [GetChapterContent: "X.Y."] to outline objectives. Set the stage briefly. Example: "Hey, {student_name}, Chapter 1.3 is verbs—sentence superstars. Ready?"
2. **Guided Reading**: Direct to a section with [BookJump: {{"chapter_number": "X.Y.", "sector_title": "Section Title"}}]. Example: "Check out the verb section in Chapter 1.3."
3. **Explanation**: Explain one concept in 2-3 sentences, using [AddMemory] for personalization. Example: "Verbs are actions, like ‘run.’ Since you love mysteries, think ‘investigate.’"
4. **Check**: Ask one question post-explanation. Example: "What’s a verb for a detective story?"
5. **Feedback**: Encourage or correct, updating [AddMemory]. Example (correct): "‘Snoop’? Nice one, sleuth!" Example (incorrect): "‘Clue’ is a noun. Try an action word."
6. **Adjust**: Move forward if understood; simplify or revisit (one [BookJump] max) if not. Log issues in [UpdateProgress].
7. **Summary**: Summarize and log with [UpdateProgress], updating [AddMemory].

## Tools:
- **GetChapterContent**: Retrieve chapter objectives and content.
- **BookJump**: Guide to textbook sections.
- **AddMemory**: Store student data for personalization.
- **UpdateProgress**: Log progress with objectives and next steps.

## Instructions:
- **Start**: Introduce Vera and {book_name} with [GetChapterContent: "1.0."]. Begin with Chapter 1.1.
- **Stay Structured**: Teach one concept at a time, using tools to plan and personalize. Guide back if off-topic.
- **Engage**: Weave in Vera’s hobbies (e.g., “Tougher than a Christie twist”).
- **Tool Invocation**: Execute tools internally; do NOT include `[ToolName: ...]` in responses. Integrate results naturally (e.g., [BookJump] becomes "Read this section").
- **Constraints**:
  - One concept, one question per step.
  - Responses must be conversational, tool-syntax-free, and tailored to {student_name}.
  - If tools fail, assume plausible content and log in [UpdateProgress].
"#
        );
        Ok(instruction)
    }

    /// return (saved_conversation, unsaved_conversation)
    pub async fn get_conversation(&self) -> anyhow::Result<Vec<ChatCompletionRequestMessage>> {
        let conversation: Vec<ChatCompletionRequestMessage> = sqlx::query_scalar!(
            "select content from history_message where student_id = ? and book_id = ? order by update_time asc",
            self.student_id,
            self.book_id
        )
        .fetch_all(&self.database)
        .await?
        .into_iter()
        .map(|content| serde_json::from_str::<ChatCompletionRequestMessage>(&content).unwrap())
        .collect();
        Ok(conversation)
    }
    pub async fn add_conversation_message(
        &self,
        message: &ChatCompletionRequestMessage,
    ) -> anyhow::Result<()> {
        let now = OffsetDateTime::now_utc();
        let content = serde_json::to_string(&message)?;
        sqlx::query!(
            "insert into history_message (student_id, book_id, content, update_time) values (?, ?, ?, ?)",
            self.student_id,
            self.book_id,
            content,
            now
        )
        .execute(&self.database)
        .await?;
        Ok(())
    }
    pub async fn add_memory(&self, memory: String) -> anyhow::Result<()> {
        let memories = sqlx::query_scalar!(
            "select memories from teacher_agent where student_id = ? and book_id = ?",
            self.student_id,
            self.book_id
        )
        .fetch_one(&self.database)
        .await?;
        let mut memories = serde_json::from_str::<BTreeSet<String>>(&memories)?;
        memories.insert(memory);
        let memories = serde_json::to_string(&memories)?;
        sqlx::query!(
            "update teacher_agent set memories = ? where student_id = ? and book_id = ?",
            memories,
            self.student_id,
            self.book_id
        )
        .execute(&self.database)
        .await?;
        Ok(())
    }
    pub async fn update_chapter_progress(
        &self,
        chapter_progress: ChapterProgress,
    ) -> anyhow::Result<ChapterProgress> {
        let chapter_number = chapter_progress.chapter_number.to_string();
        let record = sqlx::query!(
            "select status, objectives, update_time from chapter_progress where student_id = ? and book_id = ? and chapter_number = ?",
            self.student_id,
            self.book_id,
            chapter_number
        )
        .fetch_optional(&self.database)
        .await?;
        let new_chapter_progress = if let Some(record) = record {
            let mut old_chapter_progress = ChapterProgress {
                chapter_number: chapter_progress.chapter_number.clone(),
                status: ChapterStatus::from(record.status),
                objectives: serde_json::from_str::<BTreeSet<ChapterObjective>>(&record.objectives)?,
                update_time: record.update_time,
            };
            old_chapter_progress.merge(chapter_progress.clone());
            old_chapter_progress
        } else {
            chapter_progress
        };
        let status = new_chapter_progress.status as i64;
        let objectives = serde_json::to_string(&new_chapter_progress.objectives)?;
        sqlx::query!(
            "insert or replace into chapter_progress (student_id, book_id, chapter_number, status, objectives, update_time) values (?, ?, ?, ?, ?, ?)",
            self.student_id,
            self.book_id,
            chapter_number,
            status,
            objectives,
            new_chapter_progress.update_time
        )
        .execute(&self.database)
        .await?;
        Ok(new_chapter_progress)
    }

    pub async fn get_book_progress(&self) -> anyhow::Result<BookProgress> {
        let record = sqlx::query!(
            "select current_chapter_number, memories, update_time from teacher_agent where student_id = ? and book_id = ?",
            self.student_id,
            self.book_id
        )
        .fetch_one(&self.database)
        .await?;
        let memories = serde_json::from_str::<BTreeSet<String>>(&record.memories)?;
        let current_learning_chapter = record.current_chapter_number.parse()?;
        let mut book_progress = BookProgress {
            current_learning_chapter,
            chapter_progress: BTreeMap::new(),
            memories,
            update_time: record.update_time,
        };
        let chapter_progresses = sqlx::query!(
            "select chapter_number, status, objectives, update_time from chapter_progress where student_id = ? and book_id = ?",
            self.student_id,
            self.book_id
        )
        .fetch_all(&self.database)
        .await?;
        for record in chapter_progresses {
            let chapter_number: ChapterNumber = record.chapter_number.parse()?;
            let chapter_progress = ChapterProgress {
                chapter_number: chapter_number.clone(),
                status: ChapterStatus::from(record.status),
                objectives: serde_json::from_str::<BTreeSet<ChapterObjective>>(&record.objectives)?,
                update_time: record.update_time,
            };
            book_progress
                .chapter_progress
                .insert(chapter_number, chapter_progress);
        }
        Ok(book_progress)
    }
}

pub struct MessagesManager {
    instruction: ChatCompletionRequestMessage,
    book_info: ChatCompletionRequestMessage,
    conversation: Vec<ChatCompletionRequestMessage>,
    token_count: u64,
    token_budget: u64,
    database: MessagesDatabase,
}

impl MessagesManager {
    pub async fn load(
        student_id: i64,
        book: &Book,
        token_budget: u64,
        database: SqlitePool,
    ) -> anyhow::Result<Self> {
        let database = MessagesDatabase::new(book.id, student_id, database).await?;

        let instruction =
            ChatCompletionRequestMessage::System(database.get_instruction().await?.into());
        let token_count = instruction.tokens();
        if token_count > token_budget / 4 {
            bail!("Instruction token: {} is too much", token_count);
        }
        let book_info = ChatCompletionRequestMessage::System(
            format!("## Book Info\n```toml\n{}\n```", toml::to_string(&book)?).into(),
        );
        let token_count = book_info.tokens();
        if token_count > token_budget / 4 {
            bail!("Book info token: {} is too much", token_count);
        }
        let conversation = database.get_conversation().await?;
        let mut messages = Self {
            instruction,
            book_info,
            conversation,
            token_count: 0,
            token_budget,
            database,
        };
        messages.update_token_count();
        messages.clean_conversation_messages();
        Ok(messages)
    }

    pub fn get_messages(&self) -> Vec<ChatCompletionRequestMessage> {
        // get system prompt
        let mut result = vec![self.instruction.clone(), self.book_info.clone()];
        result.extend(self.conversation.clone());
        result
    }

    fn update_token_count(&mut self) {
        let mut token_count = 0;
        token_count += self.instruction.tokens();
        token_count += self.book_info.tokens();
        for message in &self.conversation {
            token_count += message.tokens();
        }
        self.token_count = token_count;
    }

    pub fn get_token_count(&self) -> u64 {
        self.token_count
    }

    pub async fn add_conversation_message(
        &mut self,
        message: impl Into<ChatCompletionRequestMessage>,
    ) -> anyhow::Result<()> {
        let message = message.into();
        self.token_count += message.tokens();
        self.database.add_conversation_message(&message).await?;
        self.conversation.push(message);
        self.clean_conversation_messages();
        Ok(())
    }

    pub async fn add_conversation_messages(
        &mut self,
        messages: impl IntoIterator<Item = impl Into<ChatCompletionRequestMessage>>,
    ) -> anyhow::Result<()> {
        for message in messages {
            self.add_conversation_message(message).await?;
        }
        Ok(())
    }

    pub fn clean_conversation_messages(&mut self) {
        while self.token_count > self.token_budget {
            let Some(message) = self.conversation.pop() else {
                break;
            };
            self.token_count -= message.tokens();
        }
    }

    pub fn get_tools(&self) -> Vec<Arc<dyn ToolDyn>> {
        vec![
            Arc::new(ProgressUpdateTool::new(self.database.clone())),
            Arc::new(AddMemoryTool::new(self.database.clone())),
            Arc::new(GetBookProgressTool::new(self.database.clone())),
        ]
    }
}
