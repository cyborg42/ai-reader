use std::{
    collections::{BTreeMap, BTreeSet, btree_map::Entry},
    hash::{DefaultHasher, Hash, Hasher},
    path::{Path, PathBuf},
};

use crate::ai_utils;

use super::chapter::{Chapter, ChapterNumber, ChapterPlan, ChapterRaw};
use anyhow::bail;
use mdbook::book;
use serde::{Deserialize, Serialize};
use tracing::{error, info};
use tree_iter::{
    iter::TreeIter,
    prelude::{DepthFirst, TreeIterMut},
};
use utoipa::ToSchema;

#[derive(Debug, Clone, Serialize, Deserialize, Default, ToSchema)]
pub struct BookTeachingPlan {
    pub teaching_plan: Option<String>,
    pub chapter_plans: BTreeMap<ChapterNumber, ChapterPlan>,
}

#[derive(Debug, Clone)]
pub struct BookRaw {
    pub id: i64,
    pub title: String,
    pub chapters: BTreeMap<ChapterNumber, ChapterRaw>,
    pub authors: Vec<String>,
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct Book {
    #[serde(skip_serializing)]
    pub id: i64,
    pub title: String,
    #[serde(skip_serializing_if = "BTreeSet::is_empty")]
    pub chapter_numbers: BTreeSet<ChapterNumber>,
    pub table_of_contents: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub authors: Vec<String>,
    pub description: Option<String>,
    pub teaching_plan: String,
    #[serde(skip_serializing)]
    pub chapters: BTreeMap<ChapterNumber, Chapter>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct BookMeta {
    pub id: i64,
    pub title: String,
    pub authors: Vec<String>,
    pub description: Option<String>,
    pub is_public: bool,
}

impl BookRaw {
    async fn load(root_dir: impl AsRef<Path>) -> anyhow::Result<BookRaw> {
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

        let mut book = BookRaw {
            id: 0,
            title,
            chapters: BTreeMap::new(),
            authors: book_cfg.authors,
            description: book_cfg.description,
        };
        let ori_book = mdbook::book::load_book(src_dir.clone(), &build_config)?;
        let mut chapters: Vec<ChapterRaw> = vec![];
        for i in ori_book.sections {
            if let book::BookItem::Chapter(ch) = i {
                chapters.push(ch.into());
            }
        }
        let mut is_prefix = true;
        let mut prefix_idx = 1;
        let mut suffix_idx = 1;
        let mut iter = TreeIterMut::<ChapterRaw, DepthFirst>::new(chapters.iter_mut());
        while let Some(mut ch) = iter.next() {
            if !ch.number.is_empty() {
                is_prefix = false;
                continue;
            }
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

    async fn generate_plan(
        &self,
        chapters: &BTreeMap<ChapterNumber, Chapter>,
    ) -> anyhow::Result<String> {
        let description = match self.description.as_ref() {
            Some(description) => format!("## Description\n{}\n\n", description),
            None => String::new(),
        };
        let mut chapter_summaries = format!(
            "# Book Title: {}\n\n{}## Chapter Summaries\n\n",
            self.title, description
        );
        for ch in chapters.values() {
            chapter_summaries.push_str(&format!(
                "### {} {}:\n{}\n\n",
                ch.number, ch.name, ch.chapter_plan.summary,
            ));
        }
        info!("generating teaching plan for book: {}", self.title);
        let prompt = r#"Generate a teaching plan for the book.
Example:
```
# Teaching Plan for "Mastering English Grammar"

## Overall Objectives
- **Primary Goal**: Enable the student to accurately understand and apply English grammar rules in written and spoken contexts.
- **Secondary Goals**:
  - Build a strong foundation in parts of speech, sentence structures, tenses, and punctuation.
  - Improve the student's ability to identify and correct grammatical mistakes.
  - Increase confidence in using complex grammar during communication.

## Learning Path
The book is divided into three stages, each designed to progressively build the student's skills:

1. **Basic Stage (Chapters 1-3)**:
   - **Focus**: Core grammar concepts (nouns, verbs, adjectives, adverbs, and simple sentences).
   - **Approach**: Interactive exercises and personalized practice.

2. **Intermediate Stage (Chapters 4-6)**:
   - **Focus**: Complex grammar topics (verb tenses, subject-verb agreement, pronouns, and clauses).
   - **Approach**: Tailored explanations and writing tasks.

3. **Advanced Stage (Chapters 7-9)**:
   - **Focus**: Advanced topics (passive voice, conditionals, reported speech, and punctuation details).
   - **Approach**: In-depth analysis and practical application.

## Teaching Strategies
- **Customized Lessons**: Adapt explanations and exercises to the student's learning pace and style.
- **Repetition and Reinforcement**: Revisit key concepts regularly to solidify understanding.
- **Feedback-Driven Approach**: Provide immediate, detailed feedback to address errors and encourage improvement.

## Assessment Methods
- **Chapter Quizzes**: Short tests after each chapter to check comprehension.
- **Comprehensive Exams**: Midterm and final tests covering multiple topics.
- **Practical Tasks**: Assignments that apply grammar rules to real-life writing or speaking scenarios.
```"#;
        let teaching_plan =
            ai_utils::summarize(&chapter_summaries, 1000, Some(prompt.to_string())).await?;
        Ok(teaching_plan)
    }

    async fn to_book(&self, book_path: impl AsRef<Path>) -> anyhow::Result<Book> {
        let teaching_plan_path = book_path.as_ref().join("teaching_plan.toml");
        let mut changed = false;
        let mut book_plan = match tokio::fs::read_to_string(&teaching_plan_path)
            .await
            .map(|s| toml::from_str::<BookTeachingPlan>(&s))
        {
            Ok(Ok(plan)) => plan,
            _ => BookTeachingPlan::default(),
        };

        let mut chapters = BTreeMap::new();
        for ch in self.iter() {
            let chapter_plan = match book_plan.chapter_plans.entry(ch.number.clone()) {
                Entry::Vacant(o) => {
                    changed = true;
                    o.insert(ch.generate_chapter_plan().await?).clone()
                }
                Entry::Occupied(o) => o.get().clone(),
            };
            let chapter = ch.to_chapter(chapter_plan);
            chapters.insert(ch.number.clone(), chapter);
        }
        let teaching_plan = match &book_plan.teaching_plan {
            Some(teaching_plan) => teaching_plan.clone(),
            None => {
                let teaching_plan = self.generate_plan(&chapters).await?;
                book_plan.teaching_plan = Some(teaching_plan.clone());
                changed = true;
                teaching_plan
            }
        };
        if changed {
            tokio::fs::write(&teaching_plan_path, toml::to_string(&book_plan)?).await?;
        }
        let book = Book {
            id: self.id,
            title: self.title.clone(),
            table_of_contents: self.get_table_of_contents(),
            authors: self.authors.clone(),
            description: self.description.clone(),
            teaching_plan,
            chapters,
            chapter_numbers: self.chapters.keys().cloned().collect(),
        };
        Ok(book)
    }

    pub fn iter(&self) -> TreeIter<'_, ChapterRaw, DepthFirst> {
        TreeIter::<ChapterRaw, DepthFirst>::new(self.chapters.values())
    }
    pub fn iter_mut(&mut self) -> TreeIterMut<'_, ChapterRaw, DepthFirst> {
        TreeIterMut::<ChapterRaw, DepthFirst>::new(self.chapters.values_mut())
    }

    pub fn get_table_of_contents(&self) -> String {
        let mut toc = format!("# {}\n", self.title);
        for ch in self.chapters.values() {
            toc.push_str(&ch.get_toc_item());
        }
        toc
    }
}

impl Book {
    pub async fn load(book_path: impl AsRef<Path>) -> anyhow::Result<Book> {
        let book_raw = BookRaw::load(&book_path).await?;
        book_raw.to_book(&book_path).await
    }
}
