use std::{
    collections::{BTreeMap, BTreeSet, btree_map::Entry},
    hash::{DefaultHasher, Hash, Hasher},
    path::{Path, PathBuf},
};

use crate::ai_utils;

use super::chapter::{Chapter, ChapterInfo, ChapterNumber, ChapterSummary};
use anyhow::bail;
use mdbook::book;
use serde::{Deserialize, Serialize};
use tracing::{error, info};
use tree_iter::{
    iter::TreeIter,
    prelude::{DepthFirst, TreeIterMut},
};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Summary {
    pub book_summary: Option<String>,
    pub chapter_summaries: BTreeMap<ChapterNumber, ChapterSummary>,
}

#[derive(Debug, Clone)]
pub struct Book {
    pub id: i64,
    pub title: String,
    pub chapters: BTreeMap<ChapterNumber, Chapter>,
    pub authors: Vec<String>,
    pub description: Option<String>,
}
#[derive(Debug, Clone, Serialize)]
pub struct BookInfo {
    #[serde(skip_serializing)]
    pub id: i64,
    pub title: String,
    #[serde(skip_serializing_if = "BTreeSet::is_empty")]
    pub chapter_numbers: BTreeSet<ChapterNumber>,
    pub table_of_contents: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub authors: Vec<String>,
    pub description: Option<String>,
    pub book_summary: String,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub chapter_infos: BTreeMap<ChapterNumber, ChapterInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BookMeta {
    pub id: i64,
    pub title: String,
    pub authors: Vec<String>,
    pub description: Option<String>,
}

impl Book {
    pub async fn load(root_dir: impl AsRef<Path>) -> anyhow::Result<Book> {
        let root_dir = root_dir.as_ref();
        info!("Loading book from {}", root_dir.display());
        let file_name = root_dir
            .with_extension("")
            .file_name()
            .ok_or(anyhow::anyhow!("invalid root dir"))?
            .to_string_lossy()
            .to_string();
        let book_toml_content = tokio::fs::read_to_string(root_dir.join("book.toml")).await?;
        let book_cfg = toml::from_str::<mdbook::config::Config>(&book_toml_content)?.book;
        let src_dir = root_dir.join(book_cfg.src);
        let build_config = mdbook::config::BuildConfig {
            build_dir: PathBuf::from(""),
            create_missing: true,
            use_default_preprocessors: true,
            extra_watch_dirs: vec![],
        };

        let title = book_cfg.title.unwrap_or(file_name);

        let mut book = Book {
            id: 0,
            title,
            chapters: BTreeMap::new(),
            authors: book_cfg.authors,
            description: book_cfg.description,
        };
        let ori_book = mdbook::book::load_book(src_dir.clone(), &build_config)?;
        let mut chapters: Vec<Chapter> = vec![];
        for i in ori_book.sections {
            if let book::BookItem::Chapter(ch) = i {
                chapters.push(ch.into());
            }
        }
        let mut is_prefix = true;
        let mut prefix_idx = 1;
        let mut suffix_idx = 1;
        let mut iter = TreeIterMut::<Chapter, DepthFirst>::new(chapters.iter_mut());
        while let Some(mut ch) = iter.next() {
            if !ch.number.is_empty() {
                is_prefix = false;
            } else {
                if is_prefix {
                    // prefix chapter, number is 0.1, 0.2, 0.3, ...
                    ch.number = ChapterNumber::from_iter(vec![0, prefix_idx]);
                    prefix_idx += 1;
                } else {
                    // suffix chapter, number is -1.1, -1.2, -1.3, ...
                    ch.number = ChapterNumber::from_iter(vec![-1, suffix_idx]);
                    suffix_idx += 1;
                }
            }
        }

        let len = chapters.len();
        book.chapters = chapters
            .into_iter()
            .map(|ch| (ch.number.clone(), ch))
            .collect();
        if book.chapters.len() != len {
            error!("chapter number is not unique, path: {}", root_dir.display());
            bail!("chapter number is not unique");
        }
        let mut hasher = DefaultHasher::new();
        book.title.hash(&mut hasher);
        book.authors.hash(&mut hasher);
        book.description.hash(&mut hasher);
        book.chapters.hash(&mut hasher);
        book.id = (hasher.finish() as i64).abs();
        Ok(book)
    }

    async fn generate_book_summary(
        &self,
        chapter_infos: &BTreeMap<ChapterNumber, ChapterInfo>,
    ) -> anyhow::Result<String> {
        let description = match self.description.as_ref() {
            Some(description) => format!("## Description\n{}\n\n", description),
            None => String::new(),
        };
        let mut summary_all = format!(
            "# Book Title: {}\n\n{}## Chapter Summary\n\n",
            self.title, description
        );
        for ch in chapter_infos.values() {
            summary_all.push_str(&format!(
                "### {} {}: \nsummary: {}\nkey_points: {}\n\n",
                ch.number,
                ch.name,
                ch.summary.summary,
                ch.summary.key_points.join(", ")
            ));
        }
        info!("generating summary for book: {}", self.title);
        let summary = ai_utils::summarize(&summary_all, 1000).await?;
        info!("generating summary for book done: {}", self.title);
        Ok(summary)
    }

    pub async fn get_book_info(&self, book_path: impl AsRef<Path>) -> anyhow::Result<BookInfo> {
        let summary_path = book_path.as_ref().join("summary.toml");
        let mut changed = false;
        let mut summary = match tokio::fs::read_to_string(&summary_path)
            .await
            .map(|s| toml::from_str::<Summary>(&s))
        {
            Ok(Ok(summary)) => summary,
            _ => Summary::default(),
        };

        let mut chapter_infos = BTreeMap::new();
        for ch in self.iter() {
            let ch_summary = match summary.chapter_summaries.entry(ch.number.clone()) {
                Entry::Vacant(o) => {
                    changed = true;
                    o.insert(ch.generate_chapter_summary().await?).clone()
                }
                Entry::Occupied(o) => o.get().clone(),
            };
            let chapter_info = ch.get_chapter_info(ch_summary);
            chapter_infos.insert(ch.number.clone(), chapter_info);
        }
        let book_summary = match &summary.book_summary {
            Some(book_summary) => book_summary.clone(),
            None => {
                let book_summary = self.generate_book_summary(&chapter_infos).await?;
                summary.book_summary = Some(book_summary.clone());
                changed = true;
                book_summary
            }
        };
        if changed {
            tokio::fs::write(&summary_path, toml::to_string(&summary)?).await?;
        }
        let book_info = BookInfo {
            id: self.id,
            title: self.title.clone(),
            table_of_contents: self.get_table_of_contents(),
            authors: self.authors.clone(),
            description: self.description.clone(),
            book_summary,
            chapter_infos,
            chapter_numbers: self.chapters.keys().cloned().collect(),
        };
        Ok(book_info)
    }

    pub fn get_chapter_content(&self, number: &ChapterNumber) -> anyhow::Result<Chapter> {
        let chapter = self
            .get_chapter(number)
            .ok_or(anyhow::anyhow!("chapter not found"))?;
        let chapter = Chapter {
            name: chapter.name.clone(),
            content: chapter.content.clone(),
            number: chapter.number.clone(),
            parent_names: chapter.parent_names.clone(),
            path: chapter.path.clone(),
            sub_chapters: vec![],
        };
        Ok(chapter)
    }
    pub fn iter(&self) -> TreeIter<'_, Chapter, DepthFirst> {
        TreeIter::<Chapter, DepthFirst>::new(self.chapters.values())
    }
    pub fn iter_mut(&mut self) -> TreeIterMut<'_, Chapter, DepthFirst> {
        TreeIterMut::<Chapter, DepthFirst>::new(self.chapters.values_mut())
    }

    pub fn get_table_of_contents(&self) -> String {
        let mut toc = format!("# {}\n", self.title);
        for ch in self.chapters.values() {
            toc.push_str(&ch.get_toc_item());
        }
        toc
    }

    pub fn get_chapter(&self, number: &ChapterNumber) -> Option<&Chapter> {
        self.chapters.get(number)
    }
}
