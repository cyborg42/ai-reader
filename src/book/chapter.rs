use std::cmp::Ordering;
use std::num::ParseIntError;
use std::path::PathBuf;
use std::str::FromStr;

use mdbook::book::{self, SectionNumber};
use schemars::JsonSchema;
use serde::Deserialize;
use serde::Serialize;
use sqlx::SqlitePool;
use std::{
    fmt::{self, Display, Formatter},
    ops::{Deref, DerefMut},
};
use tree_iter::iter::TreeNode;
use tree_iter::prelude::TreeNodeMut;

use crate::ai_utils;

#[derive(Debug, Clone, Default, Serialize, Hash)]
pub struct Chapter {
    pub name: String,
    pub number: ChapterNumber,
    pub parent_names: Vec<String>,
    pub path: Option<PathBuf>,
    pub content: String,
    #[serde(skip_serializing)]
    pub sub_chapters: Vec<Chapter>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ChapterInfo {
    pub name: String,
    pub number: ChapterNumber,
    pub parent_names: Vec<String>,
    pub path: Option<PathBuf>,
    pub summary: String,
    pub key_points: Vec<String>,
}

impl Chapter {
    pub async fn get_chapter_summary(
        &self,
        book_id: i64,
        database: &SqlitePool,
    ) -> anyhow::Result<(String, Vec<String>)> {
        let number = self.number.to_string();
        let summary = sqlx::query!(
            "SELECT summary, key_points FROM chapter WHERE book_id = ? AND chapter_number = ?",
            book_id,
            number
        )
        .fetch_optional(database)
        .await?;
        if let Some(record) = summary {
            return Ok((record.summary, serde_json::from_str(&record.key_points)?));
        }
        let summary = ai_utils::summarize(&self.content, 100).await?;
        let key_points = ai_utils::extract_key_points(&self.content).await?;
        let number = self.number.to_string();
        let key_points_str = serde_json::to_string(&key_points)?;
        sqlx::query!(
            "REPLACE INTO chapter (book_id, chapter_number, name, summary, key_points) VALUES (?, ?, ?, ?, ?)",
            book_id,
            number,
            self.name,
            summary,
            key_points_str
        )
        .execute(database)
        .await?;
        Ok((summary, key_points))
    }

    pub async fn get_chapter_info(
        &self,
        book_id: i64,
        database: &SqlitePool,
    ) -> anyhow::Result<ChapterInfo> {
        let (summary, key_points) = self.get_chapter_summary(book_id, database).await?;
        Ok(ChapterInfo {
            name: self.name.clone(),
            number: self.number.clone(),
            parent_names: self.parent_names.clone(),
            path: self.path.clone(),
            summary,
            key_points,
        })
    }
}

impl Chapter {
    pub fn get_toc_item(&self) -> String {
        let indent = if let Some(i) = self.number.0.first() {
            if [0, -1].contains(i) {
                0
            } else {
                self.number.0.len() - 1
            }
        } else {
            0
        };
        let indent = "  ".repeat(indent);
        let path = if let Some(path) = &self.path {
            path.to_str().unwrap_or("")
        } else {
            ""
        };
        let mut s = format!("{indent}{} [{}]({path})  \n", self.number, self.name,);
        for sub in &self.sub_chapters {
            s.push_str(&sub.get_toc_item());
        }
        s
    }
}

impl From<book::Chapter> for Chapter {
    fn from(ch: book::Chapter) -> Self {
        let mut chapter = Chapter {
            name: ch.name,
            content: ch.content,
            number: ch.number.unwrap_or_default().into(),
            parent_names: ch.parent_names,
            path: ch.path,
            sub_chapters: vec![],
        };
        for i in ch.sub_items {
            if let book::BookItem::Chapter(ch) = i {
                chapter.sub_chapters.push(ch.into());
            }
        }
        chapter
    }
}

impl TreeNode for Chapter {
    fn children(&self) -> impl DoubleEndedIterator<Item = &Self> {
        self.sub_chapters.iter()
    }
}

impl TreeNodeMut for Chapter {
    fn children_mut(&mut self) -> impl DoubleEndedIterator<Item = &mut Self> {
        self.sub_chapters.iter_mut()
    }
}
/// A section number like "1.2.3."
#[derive(Debug, PartialEq, Clone, Default, Eq, Hash)]
pub struct ChapterNumber(pub Vec<i64>);
impl JsonSchema for ChapterNumber {
    fn schema_name() -> String {
        "ChapterNumber".to_string()
    }
    fn json_schema(generator: &mut schemars::r#gen::SchemaGenerator) -> schemars::schema::Schema {
        // Create a schema for a string that represents a chapter number
        // The schema should validate that the string is in the format "1.2.3."
        let mut schema = String::json_schema(generator);
        // Add description to explain the format
        if let schemars::schema::Schema::Object(obj) = &mut schema {
            obj.metadata = Some(Box::new(schemars::schema::Metadata {
                description: Some("A chapter number in the format '1.2.3.' representing the hierarchical position in a book".to_string()),
                ..Default::default()
            }));

            // Add pattern to validate the format (optional numbers separated by dots)
            obj.string = Some(Box::new(schemars::schema::StringValidation {
                pattern: Some(r"^(\d+\.)+$".to_string()),
                ..Default::default()
            }));
        }
        schema
    }
}

#[test]
fn t() {
    let s = serde_json::json!(schemars::schema_for!(ChapterNumber));
    println!("{:#?}", s);
}

impl Display for ChapterNumber {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        for item in &self.0 {
            write!(f, "{item}.")?;
        }
        Ok(())
    }
}
impl Serialize for ChapterNumber {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}
impl<'de> Deserialize<'de> for ChapterNumber {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        s.parse::<ChapterNumber>().map_err(serde::de::Error::custom)
    }
}

impl FromStr for ChapterNumber {
    type Err = ParseIntError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let number: Result<Vec<i64>, Self::Err> =
            s.split_terminator('.').map(|x| x.parse()).collect();
        Ok(ChapterNumber(number?))
    }
}

impl Deref for ChapterNumber {
    type Target = Vec<i64>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for ChapterNumber {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl FromIterator<i64> for ChapterNumber {
    fn from_iter<I: IntoIterator<Item = i64>>(it: I) -> Self {
        ChapterNumber(it.into_iter().collect())
    }
}

impl From<SectionNumber> for ChapterNumber {
    fn from(number: SectionNumber) -> Self {
        ChapterNumber(number.0.into_iter().map(|x| x as i64).collect())
    }
}

impl PartialOrd for ChapterNumber {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for ChapterNumber {
    fn cmp(&self, other: &Self) -> Ordering {
        // if self.0[0] == -1, it is a suffix chapter
        match (self.0.get(0), other.0.get(0)) {
            (Some(n), Some(m)) => {
                if (*n == -1) == (*m == -1) {
                    self.0.cmp(&other.0)
                } else if *n != -1 && *m == -1 {
                    Ordering::Less
                } else {
                    Ordering::Greater
                }
            }
            _ => self.0.cmp(&other.0),
        }
    }
}

#[test]
fn chapter_number_cmp() {
    let mut set: std::collections::BTreeSet<ChapterNumber> = std::collections::BTreeSet::new();
    set.insert("3.1.4".parse().unwrap());
    set.insert("".parse().unwrap());
    set.insert("2.3.2".parse().unwrap());
    set.insert("-1.3.8.".parse().unwrap());
    set.insert("5.4.1".parse().unwrap());
    set.insert("4.7.6".parse().unwrap());
    println!("{:?}", set);
}
