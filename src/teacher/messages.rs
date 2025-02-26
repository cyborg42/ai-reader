use std::mem::take;

use anyhow::bail;
use openai::chat::{ChatCompletionMessage, ChatCompletionMessageRole};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use time::OffsetDateTime;
use tracing::warn;

use crate::{
    book::{book::BookInfo, chapter::ChapterNumber},
    llm_fn,
};

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[repr(i64)]
pub enum ChapterStatus {
    NotStarted = 0,
    InProgress = 1,
    Completed = 2,
}
impl From<i64> for ChapterStatus {
    fn from(value: i64) -> Self {
        match value {
            0 => ChapterStatus::NotStarted,
            1 => ChapterStatus::InProgress,
            2 => ChapterStatus::Completed,
            _ => ChapterStatus::NotStarted,
        }
    }
}
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct ChapterProgress {
    pub chapter_number: ChapterNumber,
    pub status: ChapterStatus,
    #[serde(skip_serializing_if = "String::is_empty", default = "String::new")]
    pub description: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct BookProgress {
    pub current_progress: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub chapter_progress: Vec<ChapterProgress>,
}

#[test]
fn test_progress() {
    use std::str::FromStr;
    let progresses = vec![
        ChapterProgress {
            chapter_number: ChapterNumber::from_str("1.2.3").unwrap(),
            status: ChapterStatus::InProgress,
            description: "just started".to_string(),
        },
        ChapterProgress {
            chapter_number: ChapterNumber::from_str("1.2.4").unwrap(),
            status: ChapterStatus::Completed,
            description: "".to_string(),
        },
    ];
    let progresses = BookProgress {
        current_progress: Some("about 50%".to_string()),
        chapter_progress: progresses,
    };
    // let progresses = BookProgress {
    //     current_progress: None,
    //     chapter_progress: vec![],
    // };
    println!("toml:\n{}", toml::to_string(&progresses).unwrap());
    println!(
        "json:\n{}",
        serde_json::to_string_pretty(&progresses).unwrap()
    );
    let mut s = llm_fn::get_json_generator();
    let schema = s.root_schema_for::<BookProgress>();
    println!(
        "schema:\n{}",
        serde_json::to_string_pretty(&schema).unwrap()
    );
}

struct MessagesDatabase {
    book_id: i64,
    student_id: i64,
    database: SqlitePool,
}

impl MessagesDatabase {
    pub fn new(book_id: i64, student_id: i64, database: SqlitePool) -> Self {
        Self {
            book_id,
            student_id,
            database,
        }
    }
    pub async fn get_instruction(&self) -> anyhow::Result<ChatCompletionMessage> {
        let student_name =
            sqlx::query_scalar!("select name from student where id = ?", self.student_id)
                .fetch_one(&self.database)
                .await?;
        let book_title = sqlx::query_scalar!("select title from book where id = ?", self.book_id)
            .fetch_one(&self.database)
            .await?;
        let instruction = ChatCompletionMessage {
            role: ChatCompletionMessageRole::System,
            content: Some(format!(
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
            )),
            ..Default::default()
        };
        Ok(instruction)
    }
    pub async fn get_study_plan(&self) -> anyhow::Result<ChatCompletionMessage> {
        let study_plan = sqlx::query_scalar!(
            "select study_plan from book_progress where student_id = ? and book_id = ?",
            self.student_id,
            self.book_id
        )
        .fetch_one(&self.database)
        .await?;
        let study_plan = ChatCompletionMessage {
            role: ChatCompletionMessageRole::System,
            content: Some(format!("# Study Plan\n{}\n", study_plan)),
            ..Default::default()
        };
        Ok(study_plan)
    }
    /// return (saved_conversation, unsaved_conversation)
    pub async fn get_conversation(
        &self,
    ) -> anyhow::Result<(Vec<ChatCompletionMessage>, Vec<ChatCompletionMessage>)> {
        let mut conversation = vec![];
        for record in  sqlx::query!(
            "select content, update_time from history_message where student_id = ? and book_id = ? order by update_time asc",
            self.student_id,
            self.book_id
        )
        .fetch_all(&self.database)
        .await?{
            let message = serde_json::from_str::<ChatCompletionMessage>(&record.content)?;
            let update_time = record.update_time;
            conversation.push((message, update_time));
        }
        let save_time = sqlx::query_scalar!(
            "select update_time from book_progress where student_id = ? and book_id = ?",
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
        message: &ChatCompletionMessage,
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
    pub async fn update_book_progress(&self, progress: BookProgress) -> anyhow::Result<()> {
        let update_time = OffsetDateTime::now_utc();
        if let Some(current_progress) = progress.current_progress {
            sqlx::query!(
                "update book_progress set current_progress = ?, update_time = ? where student_id = ? and book_id = ?",
                current_progress,
                update_time,
                self.student_id,
                self.book_id
            )
            .execute(&self.database)
            .await?;
        } else {
            sqlx::query!(
                "update book_progress set update_time = ? where student_id = ? and book_id = ?",
                update_time,
                self.student_id,
                self.book_id
            )
            .execute(&self.database)
            .await?;
        }

        for ch in progress.chapter_progress {
            let status = ch.status as i64;
            let number = ch.chapter_number.to_string();
            sqlx::query!(
                "REPLACE INTO chapter_progress (status, description, student_id, book_id, chapter_number, update_time) VALUES (?, ?, ?, ?, ?, ?)",
                status,
                ch.description,
                self.student_id,
                self.book_id,
                number,
                update_time
            )
            .execute(&self.database)
            .await?;
        }
        Ok(())
    }
    pub async fn update_study_plan(&self, study_plan: String) -> anyhow::Result<()> {
        sqlx::query!(
            "update book_progress set study_plan = ? where student_id = ? and book_id = ?",
            study_plan,
            self.student_id,
            self.book_id
        )
        .execute(&self.database)
        .await?;
        Ok(())
    }
    pub async fn get_progress(&self) -> anyhow::Result<ChatCompletionMessage> {
        let current_progress = sqlx::query_scalar!(
            "select current_progress from book_progress where student_id = ? and book_id = ?",
            self.student_id,
            self.book_id
        )
        .fetch_one(&self.database)
        .await?;
        let current_progress = if current_progress.is_empty() {
            None
        } else {
            Some(current_progress)
        };

        let mut chapter_progress = vec![];
        for record in sqlx::query!(
            "select chapter_number, status, description from chapter_progress where student_id = ? and book_id = ?",
            self.student_id,
            self.book_id
        )
        .fetch_all(&self.database)
        .await?{
            let chapter_number = record.chapter_number.parse()?;
            chapter_progress.push(ChapterProgress {
                chapter_number,
                status: ChapterStatus::from(record.status),
                description: record.description.clone(),
            });
        }

        let progress = BookProgress {
            current_progress,
            chapter_progress,
        };
        let progress = ChatCompletionMessage {
            role: ChatCompletionMessageRole::System,
            content: Some(format!(
                "## Book Progress\n```toml\n{}\n```\n\n\
                Note: This progress summary reflects conversations up to this point, \
                not including any messages that follow.",
                toml::to_string(&progress).unwrap()
            )),
            ..Default::default()
        };
        Ok(progress)
    }
}

pub struct MessagesManager {
    instruction: ChatCompletionMessage,
    book_info: ChatCompletionMessage,
    study_plan: ChatCompletionMessage,
    book_progress: ChatCompletionMessage,
    saved_conversation: Vec<ChatCompletionMessage>,
    unsaved_conversation: Vec<ChatCompletionMessage>,
    token_count: usize,
    token_budget: usize,
    database: MessagesDatabase,
}

impl MessagesManager {
    pub async fn init_with_study_plan(
        student_id: i64,
        book_info: BookInfo,
        token_budget: usize,
        study_plan: String,
        database: SqlitePool,
    ) -> anyhow::Result<Self> {
        let now = OffsetDateTime::now_utc();
        sqlx::query!(
            "insert into book_progress (book_id, student_id, study_plan, current_progress, update_time) values (?, ?, ?, '', ?)",
            book_info.id,
            student_id,
            study_plan,
            now
        )
        .execute(&database)
        .await?;
        Self::new(student_id, book_info, token_budget, database).await
    }
    pub async fn new(
        student_id: i64,
        mut book_info: BookInfo,
        token_budget: usize,
        database: SqlitePool,
    ) -> anyhow::Result<Self> {
        let book_id = book_info.id;
        let database = MessagesDatabase::new(book_id, student_id, database);
        let instruction = database.get_instruction().await?;
        let mut book_info_str = toml::to_string(&book_info)?;
        let book_info_token = llm_fn::token_count(&book_info_str);
        if book_info_token > token_budget / 4 {
            warn!(
                "Book info token: {} is too much, elimate chapter infos",
                book_info_token
            );
            book_info.chapter_infos = vec![];
            book_info_str = toml::to_string(&book_info)?;
        }
        let book_info = ChatCompletionMessage {
            role: ChatCompletionMessageRole::System,
            content: Some(format!("## Book Info\n```toml\n{}\n```", book_info_str)),
            ..Default::default()
        };
        let count = llm_fn::message_token_count(&book_info);
        if count > token_budget / 4 {
            bail!("Book info token: {} is too much", count);
        }
        let study_plan = database.get_study_plan().await?;
        let count = llm_fn::message_token_count(&study_plan);
        if count > token_budget / 4 {
            bail!("Study plan token: {} is too much", count);
        }
        let book_progress = database.get_progress().await?;
        let count = llm_fn::message_token_count(&book_progress);
        if count > token_budget / 4 {
            bail!("Book progress token: {} is too much", count);
        }
        let (saved_conversation, unsaved_conversation) = database.get_conversation().await?;
        let mut messages = Self {
            instruction,
            book_info,
            study_plan,
            book_progress,
            saved_conversation,
            unsaved_conversation,
            token_count: 0,
            token_budget,
            database,
        };
        messages.update_token_count();
        messages.clean_conversation_messages().await?;
        Ok(messages)
    }

    pub fn get_messages(&self) -> Vec<ChatCompletionMessage> {
        // get system prompt
        let mut result = vec![
            self.instruction.clone(),
            self.book_info.clone(),
            self.study_plan.clone(),
        ];
        result.extend(self.saved_conversation.clone());
        result.push(self.book_progress.clone());
        result.extend(self.unsaved_conversation.clone());
        result
    }

    pub async fn update_study_plan(&mut self, study_plan: String) -> anyhow::Result<()> {
        self.database.update_study_plan(study_plan).await?;
        let study_plan = self.database.get_study_plan().await?;
        let new_plan_token = llm_fn::message_token_count(&study_plan);
        let old_plan_token = llm_fn::message_token_count(&self.study_plan);
        self.study_plan = study_plan;
        self.token_count += new_plan_token - old_plan_token;
        self.clean_conversation_messages().await?;
        if new_plan_token > self.token_budget / 4 {
            bail!("Study plan token: {} is too much", new_plan_token);
        }
        Ok(())
    }

    fn update_token_count(&mut self) {
        let mut token_count = 0;
        token_count += llm_fn::message_token_count(&self.instruction);
        token_count += llm_fn::message_token_count(&self.book_info);
        token_count += llm_fn::message_token_count(&self.study_plan);
        token_count += llm_fn::message_token_count(&self.book_progress);
        for message in &self.saved_conversation {
            token_count += llm_fn::message_token_count(message);
        }
        for message in &self.unsaved_conversation {
            token_count += llm_fn::message_token_count(message);
        }
        self.token_count = token_count;
    }

    pub fn get_token_count(&self) -> usize {
        self.token_count
    }

    /// return (message count, token count)
    pub fn get_unsaved_msg_count(&self) -> (usize, usize) {
        let mut token_count = 0;
        for message in &self.unsaved_conversation {
            token_count += llm_fn::message_token_count(message);
        }
        (self.unsaved_conversation.len(), token_count)
    }

    pub async fn add_conversation_message(
        &mut self,
        message: ChatCompletionMessage,
    ) -> anyhow::Result<()> {
        self.token_count += llm_fn::message_token_count(&message);
        self.database.add_conversation_message(&message).await?;
        self.unsaved_conversation.push(message);
        self.clean_conversation_messages().await?;
        Ok(())
    }

    pub async fn save_to_book_progress(&mut self, progress: BookProgress) -> anyhow::Result<()> {
        self.database.update_book_progress(progress).await?;
        // move unsaved conversation to saved conversation
        self.saved_conversation
            .extend(take(&mut self.unsaved_conversation));
        let book_progress = self.database.get_progress().await?;
        let old_progress_token = llm_fn::message_token_count(&self.book_progress);
        let new_progress_token = llm_fn::message_token_count(&book_progress);
        self.book_progress = book_progress;
        self.token_count += new_progress_token - old_progress_token;
        self.clean_saved_messages_only()?;
        if new_progress_token > self.token_budget / 4 {
            bail!("Book progress token: {} is too much", new_progress_token);
        }
        Ok(())
    }

    pub async fn save_progress(&mut self) -> anyhow::Result<()> {
        let messages = self.get_messages();
        let progress = llm_fn::summarize_progress(messages, 100).await?;
        self.save_to_book_progress(progress).await?;
        Ok(())
    }

    pub fn clean_saved_messages_only(&mut self) -> anyhow::Result<()> {
        while self.token_count > self.token_budget {
            if let Some(message) = self.saved_conversation.pop() {
                self.token_count -= llm_fn::message_token_count(&message);
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
        if self.clean_saved_messages_only().is_ok() {
            return Ok(());
        }
        // if not enough, save progress and clean again
        self.save_progress().await?;
        Ok(())
    }
}
