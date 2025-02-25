mod book;
mod chapter;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use book::Book;

use sqlx::SqlitePool;
use tracing::error;

use crate::llm_fn;
#[derive(Debug, Clone)]
pub struct BookServer {
    books: HashMap<i64, Book>,
    bookbase: PathBuf,
    database: SqlitePool,
}

impl Default for BookServer {
    fn default() -> Self {
        let database = SqlitePool::connect_lazy("sqlite::memory:").unwrap();
        Self {
            books: HashMap::new(),
            bookbase: PathBuf::new(),
            database,
        }
    }
}

impl BookServer {
    /// create a new book server
    pub async fn new(
        database: impl AsRef<Path>,
        bookbase: impl AsRef<Path>,
    ) -> anyhow::Result<Self> {
        let database = SqlitePool::connect(&database.as_ref().to_string_lossy())
            .await
            .unwrap();
        let mut server = Self {
            books: HashMap::new(),
            bookbase: bookbase.as_ref().to_path_buf(),
            database,
        };
        server.load_books_from_db().await?;
        Ok(server)
    }
    /// load books from database
    async fn load_books_from_db(&mut self) -> anyhow::Result<()> {
        let books = sqlx::query!("select id, path from book")
            .fetch_all(&self.database)
            .await
            .unwrap();
        for r in books {
            let book = Book::load(self.bookbase.join(r.path)).await?;
            self.books.insert(r.id, book);
        }
        Ok(())
    }
    /// add a book to database
    async fn add_book(&mut self, path: impl AsRef<Path>) -> anyhow::Result<()> {
        let book = Book::load(&self.bookbase.join(&path)).await?;
        let authors = book.authors.join(",");
        let path = path.as_ref().to_string_lossy().to_string();
        let book_id = sqlx::query!(
            "insert into book (title, author, path, description, summary) values (?, ?, ?, ?, ?)",
            book.title,
            authors,
            path,
            book.description,
            Option::<String>::None
        )
        .execute(&self.database)
        .await?
        .last_insert_rowid();

        let mut summary = book
            .title
            .clone()
            .map(|t| format!("# {}\n", t))
            .unwrap_or_default();
        for ch in book.iter() {
            let index_number = ch.chapter.number.to_string();
            let ch_summary = llm_fn::summarize(&ch.chapter.content, 100).await?;
            summary.push_str(&format!(
                "{} - {}:\n{}\n",
                index_number, ch.chapter.name, ch_summary
            ));
            sqlx::query!(
                "insert into chapter (book_id, index_number, name, summary) values (?, ?, ?, ?)",
                book_id,
                index_number,
                ch.chapter.name,
                ch_summary
            )
            .execute(&self.database)
            .await?;
        }
        let summary = llm_fn::summarize(&summary, 1000).await?;
        sqlx::query!("update book set summary = ? where id = ?", summary, book_id)
            .execute(&self.database)
            .await?;
        self.books.insert(book_id, book);
        Ok(())
    }

    async fn add_all_books(&mut self) -> anyhow::Result<()> {
        let mut paths = Vec::new();
        for entry in walkdir::WalkDir::new(&self.bookbase) {
            let entry = entry?;
            if entry.file_name() == "book.toml" {
                if let Ok(rel_path) = entry.path().parent().unwrap().strip_prefix(&self.bookbase) {
                    let rel_path = rel_path.to_path_buf();
                    paths.push(rel_path);
                }
            }
        }

        for path in paths {
            if let Err(e) = self.add_book(self.bookbase.join(&path)).await {
                error!("add book {} failed: {}", path.display(), e);
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::{book::Book, config::OpenAIConfig, llm_fn::OPENAI_API_KEY, utils::init_log};

    #[tokio::test]
    async fn test_load_book() {
        let _guard = init_log(None);

        let key = std::fs::read_to_string("./openai_api_key.toml").unwrap();
        let key: openai::Credentials = toml::from_str::<OpenAIConfig>(&key).unwrap().into();
        OPENAI_API_KEY.set(key).unwrap();

        let book = Book::load("./test-book/src").await.unwrap();
        let toc = book.get_table_of_contents(true);
        let words = toc.split_whitespace().count();
        println!("{}", toc);
        println!("words: {}", words);
    }
}
