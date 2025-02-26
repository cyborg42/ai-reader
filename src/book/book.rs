use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
};

use super::chapter::{ChapterInfo, Chapter, ChapterNumber};
use anyhow::bail;
use mdbook::book;
use serde::Serialize;
use tracing::{error, info};
use tree_iter::{
    iter::TreeIter,
    prelude::{DepthFirst, TreeIterMut},
};

#[derive(Debug, Clone, Default)]
pub struct Book {
    pub title: String,
    pub chapters: Vec<Chapter>,
    pub chapter_numbers: Vec<ChapterNumber>,
    pub authors: Vec<String>,
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct BookInfo {
    pub id: i64,
    pub title: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub chapter_numbers: Vec<ChapterNumber>,
    pub table_of_contents: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub authors: Vec<String>,
    pub description: Option<String>,
    pub book_summary: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub chapter_infos: Vec<ChapterInfo>,
}

impl Book {
    pub async fn load(root_dir: impl AsRef<Path>) -> anyhow::Result<Self> {
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

        let mut book = Self {
            title,
            chapters: vec![],
            chapter_numbers: vec![],
            authors: book_cfg.authors,
            description: book_cfg.description,
        };
        let ori_book = mdbook::book::load_book(src_dir.clone(), &build_config)?;
        for i in ori_book.sections {
            if let book::BookItem::Chapter(ch) = i {
                book.chapters.push(ch.into());
            }
        }
        let mut is_prefix = true;
        let mut prefix_idx = 1;
        let mut suffix_idx = 1;
        let mut chapter_numbers = vec![];
        let mut iter = book.iter_mut();
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
            chapter_numbers.push(ch.number.clone());
        }
        let number_set = chapter_numbers.iter().collect::<BTreeSet<_>>();
        if number_set.len() != chapter_numbers.len() {
            error!("chapter number is not unique, path: {}", root_dir.display());
            bail!("chapter number is not unique");
        }
        book.chapter_numbers = chapter_numbers;

        Ok(book)
    }

    pub fn iter(&self) -> TreeIter<'_, Chapter, DepthFirst> {
        TreeIter::<Chapter, DepthFirst>::new(self.chapters.iter())
    }
    pub fn iter_mut(&mut self) -> TreeIterMut<'_, Chapter, DepthFirst> {
        TreeIterMut::<Chapter, DepthFirst>::new(self.chapters.iter_mut())
    }

    pub fn get_table_of_contents(&self) -> String {
        let mut toc = format!("# {}\n", self.title);
        for ch in &self.chapters {
            toc.push_str(&ch.get_toc_item());
        }
        toc
    }

    pub fn get_chapter(&self, number: &ChapterNumber) -> Option<&Chapter> {
        self.iter().find(|&ch| ch.number == *number)
    }
}
