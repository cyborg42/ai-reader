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

use crate::llm_fn;

#[derive(Debug, Clone, Default, Serialize)]
pub struct Chapter {
    pub name: String,
    pub number: ChapterNumber,
    pub parent_names: Vec<String>,
    pub path: Option<PathBuf>,
    pub content: String,
    #[serde(skip_serializing)]
    pub sub_nodes: Vec<Chapter>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ChapterInfo {
    pub name: String,
    pub number: ChapterNumber,
    pub parent_names: Vec<String>,
    pub path: Option<PathBuf>,
    pub chapter_summary: String,
}

impl Chapter {
    pub async fn get_chapter_summary(
        &self,
        book_id: i64,
        database: &SqlitePool,
    ) -> anyhow::Result<String> {
        let number = self.number.to_string();
        let summary = sqlx::query_scalar!(
            "SELECT summary FROM chapter WHERE book_id = ? AND chapter_number = ?",
            book_id,
            number
        )
        .fetch_optional(database)
        .await?;
        if let Some(summary) = summary {
            return Ok(summary);
        }
        let summary = llm_fn::summarize(&self.content, 100).await?;
        sqlx::query!(
            "REPLACE INTO chapter (book_id, chapter_number, name, summary) VALUES (?, ?, ?, ?)",
            book_id,
            number,
            self.name,
            summary
        )
        .execute(database)
        .await?;
        Ok(summary)
    }
}

impl Chapter {
    pub fn get_toc_item(&self) -> String {
        let indent = if self.number.0.len() > 1 && self.number.0[0] > 0 {
            self.number.0.len() - 1
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
        for sub in &self.sub_nodes {
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
            sub_nodes: vec![],
        };
        for i in ch.sub_items {
            if let book::BookItem::Chapter(ch) = i {
                chapter.sub_nodes.push(ch.into());
            }
        }
        chapter
    }
}

impl TreeNode for Chapter {
    fn children(&self) -> impl DoubleEndedIterator<Item = &Self> {
        self.sub_nodes.iter()
    }
}

impl TreeNodeMut for Chapter {
    fn children_mut(&mut self) -> impl DoubleEndedIterator<Item = &mut Self> {
        self.sub_nodes.iter_mut()
    }
}
/// A section number like "1.2.3."
#[derive(Debug, PartialEq, Clone, Default, Eq, JsonSchema)]
pub struct ChapterNumber(pub Vec<i64>);

impl Display for ChapterNumber {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        if self.0.is_empty() {
            write!(f, "0.")
        } else {
            for item in &self.0 {
                write!(f, "{item}.")?;
            }
            Ok(())
        }
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
        ChapterNumber::from_str(&s).map_err(serde::de::Error::custom)
    }
}

impl FromStr for ChapterNumber {
    type Err = ParseIntError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let number: Result<Vec<i64>, Self::Err> =
            s.split_terminator('.').map(|x| x.parse::<i64>()).collect();
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
                if *n != -1 && *m != -1 {
                    self.0.cmp(&other.0)
                } else if *n == -1 && *m == -1 {
                    self.0.cmp(&other.0)
                } else if *n != -1 && *m == -1 {
                    Ordering::Less
                } else {
                    Ordering::Greater
                }
            }
            (Some(_), None) => Ordering::Greater,
            (None, Some(_)) => Ordering::Less,
            (None, None) => Ordering::Equal,
        }
    }
}
