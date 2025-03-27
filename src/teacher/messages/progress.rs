use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};

use crate::utils::now_local;
use crate::{
    ai_utils::{AI_CLIENT, AI_MODEL, extract_tool},
    book::chapter::ChapterNumber,
};
use async_openai::types::{
    ChatCompletionNamedToolChoice, ChatCompletionRequestMessage, ChatCompletionToolChoiceOption,
    ChatCompletionToolType, CreateChatCompletionRequestArgs, FunctionName,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use time::serde::format_description;
#[derive(Debug, Clone, Copy, Deserialize, Serialize, Hash, JsonSchema)]
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

format_description!(
    ser_time,
    OffsetDateTime,
    "[year]-[month]-[day] [hour]:[minute]:[second]"
);

/// Represents a specific learning objective within a chapter
/// Contains the objective description and whether it has been completed
#[derive(Debug, Clone, Deserialize, Serialize, Hash, JsonSchema)]
pub struct ChapterObjective {
    /// The text description of the learning objective
    pub description: String,
    /// Whether the objective has been completed
    pub completed: bool,
    /// The current progress of the objective, don't set if the objective is completed
    pub progress: Option<String>,
    /// Next step to help the student understand the objective, don't set if the objective is completed
    pub next_step: Option<String>,
    /// The time when the objective was last updated
    #[serde(default = "now_local", with = "ser_time")]
    #[schemars(skip)]
    pub update_time: OffsetDateTime,
}
impl Ord for ChapterObjective {
    fn cmp(&self, other: &Self) -> Ordering {
        self.description.cmp(&other.description)
    }
}
impl PartialOrd for ChapterObjective {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl PartialEq for ChapterObjective {
    fn eq(&self, other: &Self) -> bool {
        self.description == other.description
    }
}
impl Eq for ChapterObjective {}

/// Tracks a student's progress through a specific chapter
#[derive(Debug, Clone, Deserialize, Serialize, Hash, JsonSchema)]
pub struct ChapterProgress {
    /// The current status of the chapter
    pub status: ChapterStatus,
    /// List of learning objectives for this chapter and their completion status
    #[serde(skip_serializing_if = "BTreeSet::is_empty", default)]
    pub objectives: BTreeSet<ChapterObjective>,
    /// The time when the chapter progress was last updated
    #[serde(default = "now_local", with = "ser_time")]
    #[schemars(skip)]
    pub update_time: OffsetDateTime,
}

impl Default for ChapterProgress {
    fn default() -> Self {
        Self {
            status: ChapterStatus::NotStarted,
            objectives: BTreeSet::new(),
            update_time: now_local(),
        }
    }
}

impl ChapterProgress {
    pub fn merge(&mut self, other: ChapterProgress) {
        self.status = other.status;
        for mut objective in other.objectives {
            if objective.completed {
                objective.progress = None;
                objective.next_step = None;
            }
            self.objectives.insert(objective);
        }
        self.update_time = other.update_time;
    }
}

/// Tracks student progress through book chapters and learning objectives
#[derive(Debug, Clone, Deserialize, Serialize, Hash, JsonSchema)]
pub struct BookProgress {
    /// The chapter number that the student is currently learning. e.g. "3.", "4.2."
    pub current_learning_chapter: ChapterNumber,
    /// Tracking progress for each chapter the student has interacted with, key is the chapter number e.g. "3.", "4.2."
    #[serde(skip_serializing_if = "BTreeMap::is_empty", default)]
    pub chapter_progress: BTreeMap<ChapterNumber, ChapterProgress>,
    /// General notes and feedback about the student's progress through the book
    #[serde(skip_serializing_if = "BTreeSet::is_empty", default)]
    pub notes: BTreeSet<String>,
    /// The time when the book progress was last updated
    #[serde(default = "now_local", with = "ser_time")]
    #[schemars(skip)]
    pub update_time: OffsetDateTime,
}

impl BookProgress {
    pub fn merge(&mut self, other: BookProgress) {
        self.current_learning_chapter = other.current_learning_chapter;
        for (chapter, progress) in other.chapter_progress {
            self.chapter_progress
                .entry(chapter)
                .or_insert_with(|| ChapterProgress::default())
                .merge(progress);
        }
        self.notes.extend(other.notes);
        self.update_time = other.update_time;
    }
    pub fn to_str(&self) -> String {
        format!(
            "## Book Progress\n```json\n{}\n```",
            serde_json::to_string(self).unwrap()
        )
    }
}

#[test]
fn t() {
    let mut settings = schemars::r#gen::SchemaSettings::default();
    settings.inline_subschemas = true;
    let mut generator = schemars::SchemaGenerator::new(settings);
    let s = BookProgress::json_schema(&mut generator);
    println!("{}", serde_json::to_string_pretty(&s).unwrap());
}

pub async fn summarize_progress(
    mut messages: Vec<ChatCompletionRequestMessage>,
) -> anyhow::Result<BookProgress> {
    let tool = extract_tool::<BookProgress>(None);
    let tool_choice = ChatCompletionToolChoiceOption::Named(ChatCompletionNamedToolChoice {
        r#type: ChatCompletionToolType::Function,
        function: FunctionName {
            name: tool.function.name.clone(),
        },
    });
    messages.push(ChatCompletionRequestMessage::System(
        "Analyze the conversation history and create a BookProgress object that ONLY includes CHANGES since the last BookProgress update. \
        Focus exclusively on: \
        1) Changes to the chapter number the student is currently studying \
        2) New progress in chapters they've engaged with since the last update \
        3) Newly completed learning objectives and conceptual milestones \
        4) New notes about their understanding \
        Do NOT include information that was already in the previous BookProgress. Only capture the \
        incremental changes to enable accurate progress tracking between sessions.".into()
    ));
    let request = CreateChatCompletionRequestArgs::default()
        .model(AI_MODEL.as_str())
        .messages(messages)
        .tools(vec![tool])
        .tool_choice(tool_choice)
        .build()
        .unwrap();
    let response = AI_CLIENT.chat().create(request).await.unwrap();
    let tool_call = response
        .choices
        .first()
        .ok_or(anyhow::anyhow!("No response from OpenAI"))?
        .message
        .tool_calls
        .as_ref()
        .and_then(|t| t.get(0).cloned())
        .ok_or(anyhow::anyhow!("No tool call"))?
        .function;
    let progress: BookProgress = serde_json::from_str(&tool_call.arguments)?;
    Ok(progress)
}

#[tokio::test]
async fn tt() {
    let messages = vec![ChatCompletionRequestMessage::User(
        "Assume I'm reading chapter 4.1 of the book 'The Rust Programming Language', \
        and I'm on the learning objective: 'Understanding Ownership'\
        I have some difficulty understanding the difference between references and borrowing. \
        Make a detailed next step to help me understand it."
            .into(),
    )];
    let progress = summarize_progress(messages).await.unwrap();
    println!("{}", serde_json::to_string_pretty(&progress).unwrap());
    let messages = vec![ChatCompletionRequestMessage::User(
        "Assume I'm reading chapter 4.1 of the book 'The Rust Programming Language', \
        and I have completed the learning objective: 'Understanding Ownership'."
            .into(),
    )];
    let progress = summarize_progress(messages).await.unwrap();
    println!("{}", serde_json::to_string_pretty(&progress).unwrap());
}
