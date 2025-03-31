pub mod progress;
use std::{
    collections::{BTreeMap, BTreeSet},
    mem::take,
};

use anyhow::bail;
use async_openai::types::ChatCompletionRequestMessage;
use progress::{BookProgress, ChapterObjective, ChapterProgress, ChapterStatus};
use sqlx::SqlitePool;
use time::OffsetDateTime;
use tracing::warn;

use crate::{book::book::BookInfo, ai_utils::Tokens};

struct MessagesDatabase {
    book_id: i64,
    student_id: i64,
    database: SqlitePool,
}

impl MessagesDatabase {
    pub async fn new(book_id: i64, student_id: i64, database: SqlitePool) -> anyhow::Result<Self> {
        sqlx::query!(
            "insert or ignore into teacher_agent (student_id, book_id, current_chapter_number, notes) values (?, ?, '', '[]')",
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
        let book_title = sqlx::query_scalar!("select title from book where id = ?", self.book_id)
            .fetch_one(&self.database)
            .await?;
        let instruction = format!(
            "You are an expert AI teaching assistant for {student_name}, who is studying '{book_title}'. Your personality is patient, encouraging, and clear.\n\n\
                Core responsibilities:\n\
                - Help understand complex concepts through examples and analogies\n\
                - Answer questions by referencing specific book content\n\
                - Use the student's progress data to provide personalized guidance\n\
                - Encourage critical thinking with Socratic questioning\n\
                - Correct misconceptions gently and clearly\n\n\
                When appropriate, suggest specific chapters or sections to review based on their progress. \
                Focus on building understanding rather than providing complete solutions. \
                Adapt your explanations to match their demonstrated knowledge level."
        );
        Ok(instruction)
    }

    /// return (saved_conversation, unsaved_conversation)
    pub async fn get_conversation(
        &self,
    ) -> anyhow::Result<(
        Vec<ChatCompletionRequestMessage>,
        Vec<ChatCompletionRequestMessage>,
    )> {
        let mut conversation = vec![];
        for record in sqlx::query!(
            "select content, update_time from history_message where student_id = ? and book_id = ? order by update_time asc",
            self.student_id,
            self.book_id
        )
        .fetch_all(&self.database)
        .await?{
            let message = serde_json::from_str::<ChatCompletionRequestMessage>(&record.content)?;
            let update_time = record.update_time;
            conversation.push((message, update_time));
        }
        let save_time = sqlx::query_scalar!(
            "select update_time from teacher_agent where student_id = ? and book_id = ?",
            self.student_id,
            self.book_id
        )
        .fetch_one(&self.database)
        .await?;
        let split_index = match conversation.binary_search_by_key(&save_time, |(_, time)| *time) {
            Ok(index) => index + 1,
            Err(index) => index,
        };
        let unsummarized_conversation = conversation.split_off(split_index);
        let conversation = conversation
            .into_iter()
            .map(|(message, _)| message)
            .collect();
        let unsummarized_conversation = unsummarized_conversation
            .into_iter()
            .map(|(message, _)| message)
            .collect();
        Ok((conversation, unsummarized_conversation))
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
    pub async fn update_book_progress(
        &self,
        book_progress_update: BookProgress,
    ) -> anyhow::Result<BookProgress> {
        let mut book_progress = self.get_book_progress().await?;
        book_progress.merge(book_progress_update);
        // Update teacher_agent table with current chapter and notes
        let current_chapter_number = book_progress.current_learning_chapter.to_string();
        let notes = serde_json::to_string(&book_progress.notes)?;
        sqlx::query!(
            "UPDATE teacher_agent SET current_chapter_number = ?, notes = ?, update_time = ? WHERE student_id = ? AND book_id = ?",
            current_chapter_number,
            notes,
            book_progress.update_time,
            self.student_id,
            self.book_id
        )
        .execute(&self.database)
        .await?;

        // Update or insert chapter progress records
        for (chapter_number, progress) in &book_progress.chapter_progress {
            let objectives = serde_json::to_string(&progress.objectives)?;
            let chapter_number = chapter_number.to_string();
            let status = progress.status as i64;
            // Use REPLACE to handle both insert and update cases
            sqlx::query!(
                "REPLACE INTO chapter_progress (student_id, book_id, chapter_number, status, objectives, update_time) VALUES (?, ?, ?, ?, ?, ?)",
                self.student_id,
                self.book_id,
                chapter_number,
                status,
                objectives,
                progress.update_time
            )
            .execute(&self.database)
            .await?;
        }
        Ok(book_progress)
    }

    pub async fn get_book_progress(&self) -> anyhow::Result<BookProgress> {
        let record = sqlx::query!(
            "select current_chapter_number, notes, update_time from teacher_agent where student_id = ? and book_id = ?",
            self.student_id,
            self.book_id
        )
        .fetch_one(&self.database)
        .await?;
        let notes = serde_json::from_str::<BTreeSet<String>>(&record.notes)?;
        let current_learning_chapter = record.current_chapter_number.parse()?;
        let mut book_progress = BookProgress {
            current_learning_chapter,
            chapter_progress: BTreeMap::new(),
            notes,
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
            let chapter_number = record.chapter_number.parse()?;
            let status = ChapterProgress {
                status: ChapterStatus::from(record.status),
                objectives: serde_json::from_str::<BTreeSet<ChapterObjective>>(&record.objectives)?,
                update_time: record.update_time,
            };
            book_progress
                .chapter_progress
                .insert(chapter_number, status);
        }
        Ok(book_progress)
    }
}

pub struct MessagesManager {
    instruction: ChatCompletionRequestMessage,
    book_info: ChatCompletionRequestMessage,
    book_progress: ChatCompletionRequestMessage,
    saved_conversation: Vec<ChatCompletionRequestMessage>,
    unsaved_conversation: Vec<ChatCompletionRequestMessage>,
    token_count: u64,
    token_budget: u64,
    // auto save unsaved messages when unsaved token count > auto_save
    auto_save: Option<u64>,
    database: MessagesDatabase,
}

impl MessagesManager {
    pub async fn load(
        student_id: i64,
        mut book_info: BookInfo,
        token_budget: u64,
        auto_save: Option<u64>,
        database: SqlitePool,
    ) -> anyhow::Result<Self> {
        let database = MessagesDatabase::new(book_info.id, student_id, database).await?;

        let instruction =
            ChatCompletionRequestMessage::System(database.get_instruction().await?.into());
        let token_count = instruction.tokens();
        if token_count > token_budget / 4 {
            bail!("Instruction token: {} is too much", token_count);
        }
        let mut book_info_str = toml::to_string(&book_info)?;
        let token_count = book_info_str.tokens();
        if token_count > token_budget / 4 {
            warn!(
                "Book info token: {} is too much, elimate chapter infos",
                token_count
            );
            book_info.chapter_infos = BTreeMap::new();
            book_info_str = toml::to_string(&book_info)?;
        }
        let book_info = ChatCompletionRequestMessage::System(
            format!("## Book Info\n```toml\n{}\n```", book_info_str).into(),
        );
        let token_count = book_info.tokens();
        if token_count > token_budget / 4 {
            bail!("Book info token: {} is too much", token_count);
        }
        let book_progress = database.get_book_progress().await?.to_str();
        let book_progress = ChatCompletionRequestMessage::System(book_progress.into());
        let token_count = book_progress.tokens();
        if token_count > token_budget / 4 {
            bail!("Book progress token: {} is too much", token_count);
        }
        let (saved_conversation, unsaved_conversation) = database.get_conversation().await?;
        let mut messages = Self {
            instruction,
            book_info,
            book_progress,
            saved_conversation,
            unsaved_conversation,
            token_count: 0,
            token_budget,
            auto_save,
            database,
        };
        messages.update_token_count();
        messages.clean_conversation_messages().await?;
        Ok(messages)
    }

    pub fn get_messages(&self) -> Vec<ChatCompletionRequestMessage> {
        // get system prompt
        let mut result = vec![self.instruction.clone(), self.book_info.clone()];
        result.extend(self.saved_conversation.clone());
        result.push(self.book_progress.clone());
        result.extend(self.unsaved_conversation.clone());
        result
    }

    fn update_token_count(&mut self) {
        let mut token_count = 0;
        token_count += self.instruction.tokens();
        token_count += self.book_info.tokens();
        token_count += self.book_progress.tokens();
        for message in &self.saved_conversation {
            token_count += message.tokens();
        }
        for message in &self.unsaved_conversation {
            token_count += message.tokens();
        }
        self.token_count = token_count;
    }

    pub fn get_token_count(&self) -> u64 {
        self.token_count
    }

    /// return (message count, token count)
    pub fn get_unsaved_msg_count(&self) -> (usize, u64) {
        let mut token_count = 0;
        for message in &self.unsaved_conversation {
            token_count += message.tokens();
        }
        (self.unsaved_conversation.len(), token_count)
    }

    pub async fn add_conversation_message(
        &mut self,
        message: impl Into<ChatCompletionRequestMessage>,
    ) -> anyhow::Result<()> {
        let message = message.into();
        self.token_count += message.tokens();
        self.database.add_conversation_message(&message).await?;
        self.unsaved_conversation.push(message);
        self.clean_conversation_messages().await?;
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

    pub async fn save_progress(&mut self) -> anyhow::Result<()> {
        let messages = self.get_messages();
        let progress = progress::summarize_progress(messages).await?;
        let book_progress = self.database.update_book_progress(progress).await?.to_str();
        // move unsaved conversation to saved conversation
        self.saved_conversation
            .extend(take(&mut self.unsaved_conversation));
        let old_progress_token = self.book_progress.tokens();
        let new_progress_token = book_progress.tokens();
        self.book_progress = ChatCompletionRequestMessage::System(book_progress.into());
        self.token_count += new_progress_token - old_progress_token;
        self.clean_saved_messages_only()?;
        if new_progress_token > self.token_budget / 4 {
            bail!("Book progress token: {} is too much", new_progress_token);
        }
        Ok(())
    }

    pub fn clean_saved_messages_only(&mut self) -> anyhow::Result<()> {
        while self.token_count > self.token_budget {
            if let Some(message) = self.saved_conversation.pop() {
                self.token_count -= message.tokens();
            } else {
                bail!(
                    "No more messages to remove, but token count is still too high, current token count: {}, token budget: {}",
                    self.token_count,
                    self.token_budget
                );
            }
        }
        Ok(())
    }

    pub async fn clean_conversation_messages(&mut self) -> anyhow::Result<()> {
        if let Some(auto_save) = self.auto_save {
            if self.get_unsaved_msg_count().1 > auto_save {
                return self.save_progress().await;
            }
        }
        if self.clean_saved_messages_only().is_ok() {
            return Ok(());
        }
        // if not enough, save progress and clean again
        self.save_progress().await
    }
}
