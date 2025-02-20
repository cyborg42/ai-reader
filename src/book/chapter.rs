use std::{collections::VecDeque, path::PathBuf};

use mdbook::book::{self, SectionNumber};

#[derive(Debug, Clone, Default)]
pub struct ChapterNode {
    pub chapter: Chapter,
    pub sub_nodes: Vec<ChapterNode>,
}

#[derive(Debug, Clone, Default)]
pub struct Chapter {
    pub name: String,
    pub content: String,
    pub number: SectionNumber,
    pub parent_names: Vec<String>,
    pub summary: Option<String>,
    pub path: Option<PathBuf>,
}

impl ChapterNode {
    pub fn get_toc_item(&self, with_summary: bool) -> String {
        let indent = if self.chapter.number.0.len() > 1 && self.chapter.number.0[0] > 0 {
            self.chapter.number.0.len() - 1
        } else {
            0
        };
        let indent = "  ".repeat(indent);
        let path = if let Some(path) = &self.chapter.path {
            path.to_str().unwrap_or("")
        } else {
            ""
        };
        let summary = if let (Some(summary), true) = (&self.chapter.summary, with_summary) {
            format!(" - {}", summary)
        } else {
            String::new()
        };
        let mut s = format!(
            "{indent}{} [{}]({}){}\n",
            self.chapter.number, self.chapter.name, path, summary
        );
        for sub in &self.sub_nodes {
            s.push_str(&sub.get_toc_item(with_summary));
        }
        s
    }
}

impl From<book::Chapter> for ChapterNode {
    fn from(ch: book::Chapter) -> Self {
        let mut chapter = ChapterNode {
            chapter: Chapter {
                name: ch.name,
                content: ch.content,
                number: ch.number.unwrap_or_default(),
                parent_names: ch.parent_names,
                summary: None,
                path: ch.path,
            },
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

pub struct Chapters<'a> {
    pub chapters: VecDeque<&'a ChapterNode>,
}

impl<'a> Iterator for Chapters<'a> {
    type Item = &'a ChapterNode;
    fn next(&mut self) -> Option<Self::Item> {
        let ch = self.chapters.pop_front()?;
        for sub in &ch.sub_nodes {
            self.chapters.push_front(sub);
        }
        Some(ch)
    }
}

pub struct ChaptersMut<'a> {
    pub chapters: VecDeque<&'a mut ChapterNode>,
}

impl<'a> Iterator for ChaptersMut<'a> {
    type Item = &'a mut Chapter;
    fn next(&mut self) -> Option<Self::Item> {
        let ch = self.chapters.pop_front()?;
        for sub_ch in &mut ch.sub_nodes {
            self.chapters.push_front(sub_ch);
        }
        Some(&mut ch.chapter)
    }
}
