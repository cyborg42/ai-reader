use std::{
    collections::{BTreeMap, BTreeSet},
    hash::{DefaultHasher, Hash, Hasher},
    path::{Path, PathBuf},
};

use crate::ai_utils;

use super::chapter::{Chapter, ChapterInfo, ChapterNumber};
use anyhow::bail;
use mdbook::book;
use serde::Serialize;
use sqlx::SqlitePool;
use tracing::{error, info};
use tree_iter::{
    iter::TreeIter,
    prelude::{DepthFirst, TreeIterMut},
};

#[derive(Debug, Clone, Default)]
pub struct BookRaw {
    pub title: String,
    pub chapters: BTreeMap<ChapterNumber, Chapter>,
    pub authors: Vec<String>,
    pub description: Option<String>,
}
impl BookRaw {
    pub fn build(self, id: i64, database: SqlitePool) -> Book {
        Book {
            id,
            title: self.title,
            chapters: self.chapters,
            authors: self.authors,
            description: self.description,
            database,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Book {
    pub id: i64,
    pub title: String,
    pub chapters: BTreeMap<ChapterNumber, Chapter>,
    pub authors: Vec<String>,
    pub description: Option<String>,
    pub database: SqlitePool,
}

impl Hash for Book {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.id.hash(state);
        self.title.hash(state);
        self.authors.hash(state);
        self.description.hash(state);
        self.chapters.hash(state);
    }
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
    #[serde(skip_serializing)]
    pub book_hash: u64,
}

impl Book {
    pub async fn load(root_dir: impl AsRef<Path>) -> anyhow::Result<BookRaw> {
        let root_dir = root_dir.as_ref();
        info!("Loading book from {}", root_dir.display());
        let file_name = root_dir
            .with_extension("")
            .file_name()
            .ok_or(anyhow::anyhow!("invalid root dir"))?
            .to_string_lossy()
            .to_string();
        let book_toml_content = std::fs::read_to_string(root_dir.join("book.toml"))?;
        let book_cfg = toml::from_str::<mdbook::config::Config>(&book_toml_content)?.book;
        let src_dir = root_dir.join(book_cfg.src);
        let build_config = mdbook::config::BuildConfig {
            build_dir: PathBuf::from(""),
            create_missing: true,
            use_default_preprocessors: true,
            extra_watch_dirs: vec![],
        };

        let title = book_cfg.title.unwrap_or(file_name);

        let mut book = BookRaw {
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
        Ok(book)
    }

    pub async fn generate_book_summary(&self) -> anyhow::Result<()> {
        let description = match self.description.as_ref() {
            Some(description) => format!("## Description\n{}\n\n", description),
            None => String::new(),
        };
        let mut summary_all = format!(
            "# Book Title: {}\n\n{}## Chapter Summary\n\n",
            self.title, description
        );
        for ch in self.iter() {
            let (ch_summary, ch_objectives) =
                ch.get_chapter_summary(self.id, &self.database).await?;
            summary_all.push_str(&format!(
                "### {} {}: \nsummary: {}\nobjectives: {}\n\n",
                ch.number,
                ch.name,
                ch_summary,
                ch_objectives.join(", ")
            ));
        }
        let summary = ai_utils::summarize(&summary_all, 1000).await?;
        sqlx::query!("update book set summary = ? where id = ?", summary, self.id)
            .execute(&self.database)
            .await?;
        Ok(())
    }

    pub async fn get_book_summary(&self) -> anyhow::Result<String> {
        let summary = sqlx::query_scalar!("select summary from book where id = ?", self.id)
            .fetch_one(&self.database)
            .await?;
        Ok(summary)
    }

    pub async fn get_book_info(&self) -> anyhow::Result<BookInfo> {
        let mut chapter_infos = BTreeMap::new();
        for ch in self.iter() {
            let chapter_info = ch.get_chapter_info(self.id, &self.database).await?;
            chapter_infos.insert(ch.number.clone(), chapter_info);
        }
        let book_summary = self.get_book_summary().await?;
        let mut hasher = DefaultHasher::new();
        self.hash(&mut hasher);
        let book_hash = hasher.finish();
        let book_info = BookInfo {
            id: self.id,
            title: self.title.clone(),
            table_of_contents: self.get_table_of_contents(),
            authors: self.authors.clone(),
            description: self.description.clone(),
            book_summary,
            chapter_infos,
            chapter_numbers: self.chapters.keys().cloned().collect(),
            book_hash,
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
