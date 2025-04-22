use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};

use crate::book::chapter::ChapterNumber;
use crate::utils::now_local;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

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
    #[serde(default = "now_local", with = "time::serde::rfc3339")]
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
    /// The chapter number that the student is currently learning. e.g. "3.", "4.2."
    pub chapter_number: ChapterNumber,
    /// The current status of the chapter
    pub status: ChapterStatus,
    /// List of learning objectives for this chapter and their completion status
    #[serde(skip_serializing_if = "BTreeSet::is_empty", default)]
    pub objectives: BTreeSet<ChapterObjective>,
    /// The time when the chapter progress was last updated
    #[serde(default = "now_local", with = "time::serde::rfc3339")]
    #[schemars(skip)]
    pub update_time: OffsetDateTime,
}

impl Default for ChapterProgress {
    fn default() -> Self {
        Self {
            chapter_number: ChapterNumber::default(),
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
    pub memories: BTreeSet<String>,
    /// The time when the book progress was last updated
    #[serde(default = "now_local", with = "time::serde::rfc3339")]
    #[schemars(skip)]
    pub update_time: OffsetDateTime,
}

#[test]
fn tt() {
    let mut chapter_progress = ChapterProgress::default();
    chapter_progress.chapter_number = "3.1".parse().unwrap();
    chapter_progress.status = ChapterStatus::InProgress;
    chapter_progress.objectives.insert(ChapterObjective {
        description: "Learn about the chapter".to_string(),
        completed: false,
        progress: Some("50%".to_string()),
        next_step: Some("Learn about the chapter".to_string()),
        update_time: now_local(),
    });
    let mut book_progress = BookProgress {
        current_learning_chapter: "3.1".parse().unwrap(),
        chapter_progress: BTreeMap::new(),
        memories: BTreeSet::new(),
        update_time: now_local(),
    };
    book_progress
        .chapter_progress
        .insert("3.1".parse().unwrap(), chapter_progress);
    let str = serde_json::to_string(&book_progress).unwrap();
    println!("{}", str);
}

impl BookProgress {
    pub fn add_memory(&mut self, memory: String) {
        self.memories.insert(memory);
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
