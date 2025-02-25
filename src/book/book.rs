use std::{
    fs::File,
    io::Read,
    path::{Path, PathBuf},
};

use super::chapter::{ChapterNode, Chapters, ChaptersMut};
use mdbook::book::{self, SectionNumber};

#[derive(Debug, Clone, Default)]
pub struct Book {
    pub title: Option<String>,
    pub chapters: Vec<ChapterNode>,
    pub index: Vec<SectionNumber>,
    pub authors: Vec<String>,
    pub description: Option<String>,
}

impl Book {
    pub async fn load(root_dir: impl AsRef<Path>) -> anyhow::Result<Self> {
        let root_dir = root_dir.as_ref().to_path_buf();
        let book_toml_content = std::fs::read_to_string(root_dir.join("book.toml"))?;
        let book_cfg = toml::from_str::<mdbook::config::Config>(&book_toml_content)?.book;
        let src_dir = book_cfg.src;
        let build_config = mdbook::config::BuildConfig {
            build_dir: PathBuf::from(""),
            create_missing: true,
            use_default_preprocessors: true,
            extra_watch_dirs: vec![],
        };

        let mut ori_book = mdbook::book::load_book(src_dir.clone(), &build_config)?;

        let summary_md = src_dir.join("SUMMARY.md");
        let mut summary_content = String::new();
        File::open(&summary_md)?.read_to_string(&mut summary_content)?;
        let title = mdbook::book::parse_summary(&summary_content)?.title;

        let mut idx = 1;
        ori_book.for_each_mut(|item| {
            if let book::BookItem::Chapter(ch) = item {
                if ch.number.is_none() {
                    ch.number = Some(SectionNumber::from_iter(vec![0, idx]));
                    idx += 1;
                }
            }
        });

        let mut book = Self {
            title,
            chapters: vec![],
            index: vec![],
            authors: book_cfg.authors,
            description: book_cfg.description,
        };
        for i in ori_book.sections {
            if let book::BookItem::Chapter(ch) = i {
                book.index.push(ch.number.clone().unwrap_or_default());
                book.chapters.push(ch.into());
            }
        }
        Ok(book)
    }

    pub fn iter(&self) -> Chapters<'_> {
        Chapters {
            chapters: self.chapters.iter().collect(),
        }
    }

    pub fn iter_mut(&mut self) -> ChaptersMut<'_> {
        ChaptersMut {
            chapters: self.chapters.iter_mut().collect(),
        }
    }

    pub fn get_table_of_contents(&self, with_summary: bool) -> String {
        let mut menu = if let Some(title) = &self.title {
            format!("# {}\n", title)
        } else {
            String::new()
        };
        for ch in &self.chapters {
            menu.push_str(&ch.get_toc_item(with_summary));
        }
        menu
    }
    pub fn get_chapter(&self, number: &SectionNumber) -> Option<&ChapterNode> {
        self.iter().find(|&ch| ch.chapter.number == *number)
    }
}
