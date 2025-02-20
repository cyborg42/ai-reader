mod chapter;

use std::{
    fs::File,
    io::Read,
    path::{Path, PathBuf},
};

use chapter::{ChapterNode, Chapters, ChaptersMut};
use mdbook::book::{self, SectionNumber};

use tracing::info;

use crate::llm_fn::{self};

#[derive(Debug, Clone, Default)]
pub struct Book {
    pub title: Option<String>,
    pub chapters: Vec<ChapterNode>,
    pub src_dir: PathBuf,
    pub store_dir: PathBuf,
    /// chapter summary limit in words, won't load summary if it's less than 10
    pub summary_limit: usize,
}

impl Book {
    pub async fn load(
        src_dir: impl AsRef<Path>,
        store_dir: impl AsRef<Path>,
        summary_limit: usize,
    ) -> anyhow::Result<Self> {
        let src_dir = src_dir.as_ref().to_path_buf();
        let store_dir = store_dir.as_ref().to_path_buf();

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
            src_dir,
            store_dir,
            summary_limit,
        };
        for i in ori_book.sections {
            if let book::BookItem::Chapter(ch) = i {
                book.chapters.push(ch.into());
            }
        }
        book.load_summary(false).await?;
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

    pub async fn load_summary(&mut self, regenerate: bool) -> anyhow::Result<()> {
        if self.summary_limit < 10 {
            return Ok(());
        }
        let src_dir = self.src_dir.clone();
        let summary_limit = self.summary_limit;
        for ch in self.iter_mut() {
            if ch.content.is_empty() {
                continue;
            }
            let path = if let Some(path) = &ch.path {
                let mut path = path.clone();
                path.set_extension("summary");
                src_dir.join(path)
            } else {
                continue;
            };
            if !regenerate {
                if let Ok(summary) = tokio::fs::read_to_string(&path).await {
                    if summary.split_whitespace().count() <= summary_limit {
                        // restore summary from file
                        info!("restore summary from file: {}", path.to_str().unwrap());
                        ch.summary = Some(summary);
                        continue;
                    }
                }
            }
            let summary = llm_fn::summarize(&ch.content, summary_limit).await?;
            tokio::fs::write(&path, summary.clone()).await?;
            info!("generate summary: {}", path.to_str().unwrap());
            ch.summary = Some(summary);
        }
        Ok(())
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

#[cfg(test)]
mod tests {
    use crate::{book::Book, config::OpenAIConfig, llm_fn::OPENAI_API_KEY, utils::init_log};

    #[test]
    fn test_load_book() {
        let _guard = init_log(None);

        let key = std::fs::read_to_string("./openai_api_key.toml").unwrap();
        let key: openai::Credentials = toml::from_str::<OpenAIConfig>(&key).unwrap().into();
        OPENAI_API_KEY.set(key).unwrap();

        let rt = tokio::runtime::Runtime::new().unwrap();
        let future = async move {
            let book = Book::load("./test-book/src", "./test-book/store", 20)
                .await
                .unwrap();
            let toc = book.get_table_of_contents(true);
            let words = toc.split_whitespace().count();
            println!("{}", toc);
            println!("words: {}", words);
        };
        rt.block_on(future);
    }
}
