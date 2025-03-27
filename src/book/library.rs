use std::path::{Path, PathBuf};

use super::book::{Book, BookInfo};
use dashmap::DashMap;
use dashmap::mapref::one::Ref;

use sqlx::SqlitePool;
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

    pub async fn get_book(&self, id: i64) -> anyhow::Result<Ref<i64, Book>> {
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
        let book = Book::load(self.bookbase.join(path))
            .await?
            .build(id, self.database.clone());
        self.books.insert(id, book);
        Ok(())
    }

    async fn load_books_from_db(&self) -> anyhow::Result<()> {
        let books = sqlx::query!("select id, path from book")
            .fetch_all(&self.database)
            .await?;
        for r in books {
            let book = match Book::load(self.bookbase.join(&r.path)).await {
                Ok(book) => book.build(r.id, self.database.clone()),
                Err(e) => {
                    error!("load book {} failed: {}", r.path, e);
                    continue;
                }
            };
            self.books.insert(r.id, book);
        }
        Ok(())
    }

    async fn delete_book_from_db(&self, book_id: i64) -> anyhow::Result<()> {
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
        let query = if replace {
            sqlx::query!(
                r#"replace into book (title, path, author, description, summary) values (?, ?, ?, ?, "")"#,
                title,
                path,
                authors,
                book.description
            )
        } else {
            // path is unique, so this will fail if the book already exists
            sqlx::query!(
                r#"insert into book (title, path, author, description, summary) values (?, ?, ?, ?, "")"#,
                title,
                path,
                authors,
                book.description
            )
        };
        let book_id = query.execute(&self.database).await?.last_insert_rowid();
        let book = book.build(book_id, self.database.clone());
        if let Err(e) = book.generate_book_summary().await {
            // if get book summary failed, delete the book from database
            self.delete_book_from_db(book_id).await?;
            return Err(e);
        }
        self.books.insert(book_id, book);
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

    pub async fn get_book_info(&self, book_id: i64) -> anyhow::Result<BookInfo> {
        let book = self.get_book(book_id).await?;
        book.get_book_info().await
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
        let database = SqlitePool::connect(":memory:").await.unwrap();
        let book = Book::load("./test-book/src")
            .await
            .unwrap()
            .build(0, database);
        let toc = book.get_table_of_contents();
        let words = toc.split_whitespace().count();
        println!("{}", toc);
        println!("words: {}", words);
    }
}
