use std::{
    collections::VecDeque,
    fs::File,
    io::Read,
    path::{Path, PathBuf},
};

use mdbook::book::{self, SectionNumber};
use tracing::info;

use crate::llm_fn::{self};

#[derive(Debug, Clone, Default)]
pub struct Chapter {
    pub name: String,
    pub content: String,
    pub number: SectionNumber,
    pub sub_items: Vec<Chapter>,
    pub parent_names: Vec<String>,
    pub summary: Option<String>,
    pub path: Option<PathBuf>,
}

pub struct ChapterMut<'a> {
    pub name: &'a mut String,
    pub content: &'a mut String,
    pub number: &'a mut SectionNumber,
    pub parent_names: &'a mut Vec<String>,
    pub summary: &'a mut Option<String>,
    pub path: &'a mut Option<PathBuf>,
}

impl Chapter {
    pub fn get_toc_item(&self, with_summary: bool) -> String {
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
        let summary = if let (Some(summary), true) = (&self.summary, with_summary) {
            format!(" - {}", summary)
        } else {
            String::new()
        };
        let mut s = format!(
            "{indent}{} [{}]({}){}\n",
            self.number, self.name, path, summary
        );
        for sub in &self.sub_items {
            s.push_str(&sub.get_toc_item(with_summary));
        }
        s
    }
}

impl From<book::Chapter> for Chapter {
    fn from(ch: book::Chapter) -> Self {
        let mut chapter = Chapter {
            name: ch.name,
            content: ch.content,
            number: ch.number.unwrap_or_default(),
            sub_items: vec![],
            parent_names: ch.parent_names,
            summary: None,
            path: ch.path,
        };
        for i in ch.sub_items {
            if let book::BookItem::Chapter(ch) = i {
                chapter.sub_items.push(ch.into());
            }
        }
        chapter
    }
}

pub struct Chapters<'a> {
    chapters: VecDeque<&'a Chapter>,
}

impl<'a> Iterator for Chapters<'a> {
    type Item = &'a Chapter;
    fn next(&mut self) -> Option<Self::Item> {
        let ch = self.chapters.pop_front()?;
        for sub in &ch.sub_items {
            self.chapters.push_front(sub);
        }
        Some(ch)
    }
}

pub struct ChaptersMut<'a> {
    chapters: VecDeque<&'a mut Chapter>,
}

impl<'a> Iterator for ChaptersMut<'a> {
    type Item = ChapterMut<'a>;
    fn next(&mut self) -> Option<Self::Item> {
        let ch = self.chapters.pop_front()?;
        for sub_ch in &mut ch.sub_items {
            self.chapters.push_front(sub_ch);
        }
        Some(ChapterMut {
            name: &mut ch.name,
            content: &mut ch.content,
            number: &mut ch.number,
            parent_names: &mut ch.parent_names,
            summary: &mut ch.summary,
            path: &mut ch.path,
        })
    }
}

#[derive(Debug, Clone, Default)]
pub struct Book {
    pub title: Option<String>,
    pub chapters: Vec<Chapter>,
    pub src_dir: PathBuf,
    /// chapter summary limit in words, won't load summary if it's less than 10
    pub summary_limit: usize,
}

impl Book {
    pub fn load(src_dir: impl AsRef<Path>, summary_limit: usize) -> anyhow::Result<Self> {
        let src_dir = src_dir.as_ref().to_path_buf();

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
            summary_limit,
        };
        for i in ori_book.sections {
            if let book::BookItem::Chapter(ch) = i {
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

    pub async fn load_summary(&mut self) -> anyhow::Result<()> {
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
            if let Ok(summary) = tokio::fs::read_to_string(&path).await {
                if summary.split_whitespace().count() <= summary_limit {
                    // restore summary from file
                    info!("restore summary from file: {}", path.to_str().unwrap());
                    *ch.summary = Some(summary);
                    continue;
                }
            }
            let summary = llm_fn::summarize(ch.content, summary_limit).await?;
            tokio::fs::write(&path, summary.clone()).await?;
            info!("generate summary: {}", path.to_str().unwrap());
            *ch.summary = Some(summary);
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
    pub fn get_chapter(&self, number: &SectionNumber) -> Option<&Chapter> {
        self.iter().find(|&ch| ch.number == *number)
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        book::Book,
        config::OpenAIConfig,
        llm_fn::{self, OPENAI_API_KEY}, utils::init_log,
    };

    #[test]
    fn test_load_book() {
        let _guard = init_log(None);
        let mut book = Book::load("./test-book/src", 20).unwrap();

        let key = std::fs::read_to_string("./openai_api_key.toml").unwrap();
        let key: openai::Credentials = toml::from_str::<OpenAIConfig>(&key).unwrap().into();
        OPENAI_API_KEY.set(key).unwrap();

        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(book.load_summary()).unwrap();
        let toc = book.get_table_of_contents(true);
        let words = toc.split_whitespace().count();
        println!("{}", toc);
        println!("words: {}", words);
    }

    #[test]
    fn test_summarize() {
        let key = std::fs::read_to_string("./openai_api_key.toml").unwrap();
        let key: openai::Credentials = toml::from_str::<OpenAIConfig>(&key).unwrap().into();
        OPENAI_API_KEY.set(key).unwrap();
        let story = "Once upon a time, there was a young programmer who loved to code. Every day, she would spend hours crafting elegant solutions to complex problems. Her passion for programming grew stronger with each line of code she wrote. One day, she created an amazing application that helped many people. The joy of seeing others benefit from her work made all the late nights worth it. She realized that programming wasn't just about writing code - it was about making a difference in the world.";
        let summary = llm_fn::summarize(story, 20);
        let rt = tokio::runtime::Runtime::new().unwrap();
        let result = rt.block_on(summary);
        let summary = result.unwrap();
        println!("{}", summary);
    }
}
