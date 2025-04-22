use std::path::{Path, PathBuf};

use super::book::{Book, BookMeta};
use anyhow::bail;
use dashmap::DashMap;
use dashmap::mapref::one::Ref;

use sqlx::SqlitePool;
use tokio::task::{block_in_place, spawn_blocking};
use tracing::{error, info};

#[derive(Debug, Clone)]
pub struct Library {
    pub books: DashMap<i64, Book>,
    pub bookbase: PathBuf,
    pub database: SqlitePool,
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
    pub async fn new(database: SqlitePool, bookbase: impl AsRef<Path>) -> anyhow::Result<Self> {
        sqlx::query!("PRAGMA foreign_keys = ON;")
            .execute(&database)
            .await?;
        let server = Self {
            books: DashMap::new(),
            bookbase: bookbase.as_ref().to_path_buf(),
            database,
        };
        server.restore_db_from_bookbase().await?;
        Ok(server)
    }

    pub async fn get_book(&self, id: i64) -> anyhow::Result<Ref<i64, Book>> {
        if let Some(book) = self.books.get(&id) {
            Ok(book)
        } else {
            self.load_book(id).await?;
            Ok(self.books.get(&id).unwrap())
        }
    }

    async fn load_book(&self, id: i64) -> anyhow::Result<()> {
        let _exist = sqlx::query_scalar!("select id from book where id = ?", id)
            .fetch_one(&self.database)
            .await?;
        let book = Book::load(self.bookbase.join(format!("book_{}", id))).await?;
        if id != book.id {
            bail!("Book ID mismatch: {} != {}", id, book.id);
        }
        self.books.insert(id, book);
        Ok(())
    }

    pub async fn load_books(&self) -> anyhow::Result<()> {
        let book_ids: Vec<i64> = sqlx::query_scalar!("select id from book")
            .fetch_all(&self.database)
            .await?;
        for id in book_ids {
            self.load_book(id).await?;
        }
        Ok(())
    }

    pub async fn delete_book(&self, book_id: i64) -> anyhow::Result<()> {
        let path = self.bookbase.join(format!("book_{}", book_id));
        sqlx::query!("delete from chapter where book_id = ?", book_id)
            .execute(&self.database)
            .await?;
        sqlx::query!("delete from book where id = ?", book_id)
            .execute(&self.database)
            .await?;
        let _ = tokio::fs::remove_dir_all(path).await;
        Ok(())
    }

    async fn store_book_to_db(&self, book: &Book) -> anyhow::Result<()> {
        let authors = book.authors.join(",");
        let description = book.description.clone().unwrap_or_default();
        sqlx::query!(
            "insert or replace into book (id, title, authors, description) values (?, ?, ?, ?)",
            book.id,
            book.title,
            authors,
            description
        )
        .execute(&self.database)
        .await?;
        sqlx::query!("delete from chapter where book_id = ?", book.id)
            .execute(&self.database)
            .await?;
        for (number, chapter) in book.chapters.iter() {
            let number = number.to_string();
            sqlx::query!(
                "insert or replace into chapter (book_id, chapter_number, name) values (?, ?, ?)",
                book.id,
                number,
                chapter.name
            )
            .execute(&self.database)
            .await?;
        }
        Ok(())
    }

    pub async fn restore_db_from_bookbase(&self) -> anyhow::Result<()> {
        let mut entries = tokio::fs::read_dir(&self.bookbase).await?;
        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let Some(Ok(book_id)) = entry
                .file_name()
                .to_string_lossy()
                .strip_prefix("book_")
                .map(|s| s.parse::<i64>())
            else {
                continue;
            };
            let existing = sqlx::query!("select id from book where id = ?", book_id)
                .fetch_optional(&self.database)
                .await?;
            if existing.is_some() {
                continue;
            }
            let book = match Book::load(&path).await {
                Ok(book) => book,
                Err(e) => {
                    error!("load book {} failed: {}", path.display(), e);
                    continue;
                }
            };
            if book.id != book_id {
                error!("Book ID mismatch: {} != {}", book_id, book.id);
                tokio::fs::remove_dir_all(&path).await?;
                continue;
            }
            self.store_book_to_db(&book).await?;
        }
        Ok(())
    }

    pub async fn upload_book_from_mdbook(&self, path: impl AsRef<Path>) -> anyhow::Result<()> {
        let path = path.as_ref();
        let book_raw = Book::load(path).await?;

        // Check if the book already exists in the database
        let existing = sqlx::query!("SELECT id FROM book WHERE id = ?", book_raw.id)
            .fetch_optional(&self.database)
            .await?;
        if existing.is_some() {
            bail!("Book with ID {} already exists", book_raw.id);
        }
        // Create the book directory in bookbase
        let book_dir = self.bookbase.join(format!("book_{}", book_raw.id));
        let _ = tokio::fs::remove_dir_all(&book_dir).await;
        tokio::fs::create_dir_all(&book_dir).await?;

        // Copy the book files from source path to bookbase/book_id
        let copy_options = fs_extra::dir::CopyOptions {
            overwrite: true,
            skip_exist: false,
            copy_inside: true,
            content_only: true,
            ..Default::default()
        };

        let path_buf = path.to_path_buf();
        spawn_blocking(move || fs_extra::dir::copy(path_buf, &book_dir, &copy_options)).await??;

        // Insert or replace book in the database
        self.store_book_to_db(&book_raw).await?;
        info!(
            "add book {}-{} from {} success",
            book_raw.id,
            book_raw.title,
            path.display()
        );
        Ok(())
    }

    pub async fn upload_book(&self, path: impl AsRef<Path>) -> anyhow::Result<()> {
        let path = path.as_ref();
        if path.is_dir() {
            self.upload_book_from_mdbook(path).await?;
        } else if path.is_file() && path.extension().unwrap_or_default() == "epub" {
            block_in_place(async || -> anyhow::Result<()> {
                let output_dir = tempfile::tempdir()?;
                epub2mdbook::convert_epub_to_mdbook(path, &output_dir, false)?;
                self.upload_book_from_mdbook(&output_dir).await?;
                Ok(())
            })
            .await?;
        } else {
            bail!("Invalid book path: {}", path.display());
        };
        Ok(())
    }

    pub async fn upload_books_in_dir(&self, dir: impl AsRef<Path>) -> anyhow::Result<()> {
        let mut entries = tokio::fs::read_dir(dir).await?;
        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            if let Err(e) = self.upload_book(&path).await {
                error!("add book {} failed: {}", path.display(), e);
            }
        }
        Ok(())
    }

    pub async fn get_book_list(&self) -> anyhow::Result<Vec<BookMeta>> {
        let books = sqlx::query!("select id, title, authors, description from book")
            .fetch_all(&self.database)
            .await?;
        let mut book_list = Vec::new();
        for book in books {
            let book_meta = BookMeta {
                id: book.id,
                title: book.title,
                authors: book.authors.split(',').map(|s| s.to_string()).collect(),
                description: book.description,
            };
            book_list.push(book_meta);
        }
        Ok(book_list)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utils::init_log;

    #[tokio::test]
    async fn test_load_books() {
        let _guard = init_log(None);
        let database = SqlitePool::connect("./database/book.db").await.unwrap();
        let server = Library::new(database, "./bookbase").await;
        let server = match server {
            Ok(server) => server,
            Err(e) => {
                println!("Error: {:?}", e);
                return;
            }
        };
        server.upload_books_in_dir("./test-book").await.unwrap();
    }
}
