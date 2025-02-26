use std::path::{Path, PathBuf};

use super::book::{Book, BookInfo};
use super::chapter::{Chapter, ChapterInfo, ChapterNumber};
use dashmap::DashMap;
use dashmap::mapref::one::Ref;

use crate::llm_fn;
use sqlx::SqlitePool;
use tracing::{error, info};

#[derive(Debug, Clone)]
pub struct Library {
    books: DashMap<i64, Book>,
    bookbase: PathBuf,
    database: SqlitePool,
}

impl Default for Library {
    fn default() -> Self {
        let database = SqlitePool::connect_lazy("sqlite::memory:").unwrap();
        Self {
            books: DashMap::new(),
            bookbase: PathBuf::new(),
            database,
        }
    }
}

impl Library {
    /// create a new book server
    pub async fn new(
        database: SqlitePool,
        bookbase: impl AsRef<Path>,
        scan_new_books: bool,
    ) -> anyhow::Result<Self> {
        let server = Self {
            books: DashMap::new(),
            bookbase: bookbase.as_ref().to_path_buf(),
            database,
        };
        server.load_books_from_db().await?;
        if scan_new_books {
            server.add_all_books_to_db(false).await?;
        }
        Ok(server)
    }

    async fn get_book(&self, id: i64) -> anyhow::Result<Ref<i64, Book>> {
        if let Some(book) = self.books.get(&id) {
            Ok(book)
        } else {
            self.load_book_from_db(id).await?;
            Ok(self.books.get(&id).unwrap())
        }
    }

    async fn load_book_from_db(&self, id: i64) -> anyhow::Result<()> {
        let path = sqlx::query_scalar!("select path from book where id = ?", id)
            .fetch_one(&self.database)
            .await?;
        let book = Book::load(self.bookbase.join(path)).await?;
        self.books.insert(id, book);
        Ok(())
    }

    async fn load_books_from_db(&self) -> anyhow::Result<()> {
        let books = sqlx::query!("select id, path from book")
            .fetch_all(&self.database)
            .await?;
        for r in books {
            let book = match Book::load(self.bookbase.join(&r.path)).await {
                Ok(book) => book,
                Err(e) => {
                    error!("load book {} failed: {}", r.path, e);
                    continue;
                }
            };
            self.books.insert(r.id.unwrap(), book);
        }
        Ok(())
    }

    async fn get_book_summary(&self, book_id: i64) -> anyhow::Result<String> {
        let summary = sqlx::query_scalar!("select summary from book where id = ?", book_id)
            .fetch_one(&self.database)
            .await?;
        Ok(summary)
    }

    async fn generate_book_summary(&self, book_id: i64) -> anyhow::Result<()> {
        let book = self.get_book(book_id).await?;
        let description = match book.description.as_ref() {
            Some(description) => format!("## Description\n{}\n\n", description),
            None => String::new(),
        };
        let mut summary_all = format!(
            "# Book Title: {}\n\n{}## Chapter Summary\n\n",
            book.title, description
        );
        for ch in book.iter() {
            let ch_summary = ch.get_chapter_summary(book_id, &self.database).await?;
            summary_all.push_str(&format!(
                "### {} {}: \n{}\n\n",
                ch.number, ch.name, ch_summary
            ));
        }
        let summary = llm_fn::summarize(&summary_all, 1000).await?;
        sqlx::query!("update book set summary = ? where id = ?", summary, book_id)
            .execute(&self.database)
            .await?;
        Ok(())
    }

    async fn delete_book_from_db(&self, book_id: i64) -> anyhow::Result<()> {
        self.books.remove(&book_id);
        sqlx::query!("delete from chapter where book_id = ?", book_id)
            .execute(&self.database)
            .await?;
        sqlx::query!("delete from book where id = ?", book_id)
            .execute(&self.database)
            .await?;
        Ok(())
    }

    async fn add_book_to_db(&self, path: impl AsRef<Path>, replace: bool) -> anyhow::Result<()> {
        let book = Book::load(&self.bookbase.join(&path)).await?;
        let title = book.title.clone();
        let authors = book.authors.join(",");
        let path = path.as_ref().to_string_lossy().to_string();
        // path is unique, so this will fail if the book already exists
        let book_id = if replace {
            sqlx::query!(
                r#"replace into book (title, path, author, description, summary) values (?, ?, ?, ?, "")"#,
                title,
                path,
                authors,
                book.description
            )
        } else {
            sqlx::query!(
                r#"insert into book (title, path, author, description, summary) values (?, ?, ?, ?, "")"#,
                title,
                path,
                authors,
                book.description
            )
        };
        let book_id = book_id.execute(&self.database).await?.last_insert_rowid();
        self.books.insert(book_id, book);
        if let Err(e) = self.generate_book_summary(book_id).await {
            // if get book summary failed, delete the book from database
            self.delete_book_from_db(book_id).await?;
            return Err(e);
        }
        info!("add book {}-{} from {} success", book_id, title, path);
        Ok(())
    }

    /// add all books in the bookbase to database
    async fn add_all_books_to_db(&self, replace: bool) -> anyhow::Result<()> {
        let mut paths = Vec::new();
        for entry in walkdir::WalkDir::new(&self.bookbase) {
            let entry = match entry {
                Ok(entry) => entry,
                Err(e) => {
                    error!("walkdir error: {}", e);
                    continue;
                }
            };
            if entry.file_name() == "book.toml" {
                if let Ok(rel_path) = entry.path().parent().unwrap().strip_prefix(&self.bookbase) {
                    let rel_path = rel_path.to_path_buf();
                    paths.push(rel_path);
                }
            }
        }
        for path in paths {
            if let Err(e) = self.add_book_to_db(&path, replace).await {
                error!("add book {} failed: {}", path.display(), e);
            }
        }
        Ok(())
    }

    pub async fn get_chapter_content(
        &mut self,
        book_id: i64,
        section_number: ChapterNumber,
    ) -> anyhow::Result<Chapter> {
        let book = self.get_book(book_id).await?;
        let chapter = book
            .get_chapter(&section_number)
            .ok_or(anyhow::anyhow!("chapter not found"))?;
        let chapter = Chapter {
            name: chapter.name.clone(),
            content: chapter.content.clone(),
            number: chapter.number.clone(),
            parent_names: chapter.parent_names.clone(),
            path: chapter.path.clone(),
            sub_nodes: vec![],
        };
        Ok(chapter)
    }

    pub async fn get_book_info(&self, book_id: i64) -> anyhow::Result<BookInfo> {
        let book = self.get_book(book_id).await?;
        let mut chapter_infos = Vec::with_capacity(book.chapters.len());
        for ch in book.iter() {
            let chapter_summary = ch.get_chapter_summary(book_id, &self.database).await?;
            chapter_infos.push(ChapterInfo {
                name: ch.name.clone(),
                number: ch.number.clone(),
                parent_names: ch.parent_names.clone(),
                path: ch.path.clone(),
                chapter_summary,
            });
        }
        let book_summary = self.get_book_summary(book_id).await?;
        let book_info = BookInfo {
            id: book_id,
            title: book.title.clone(),
            table_of_contents: book.get_table_of_contents(),
            authors: book.authors.clone(),
            description: book.description.clone(),
            book_summary,
            chapter_infos,
            chapter_numbers: book.chapter_numbers.clone(),
        };
        Ok(book_info)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{book::library::Book, utils::init_log};

    #[tokio::test]
    async fn test_load_books() {
        let _guard = init_log(None);
        let database = SqlitePool::connect("./database/book.db").await.unwrap();
        let server = Library::new(database, "./test-book", false).await;
        let server = match server {
            Ok(server) => server,
            Err(e) => {
                println!("Error: {:?}", e);
                return;
            }
        };
        server.add_all_books_to_db(false).await.unwrap();
    }

    #[tokio::test]
    async fn test_load_book() {
        let _guard = init_log(None);
        let book = Book::load("./test-book/src").await.unwrap();
        let toc = book.get_table_of_contents();
        let words = toc.split_whitespace().count();
        println!("{}", toc);
        println!("words: {}", words);
    }
}
