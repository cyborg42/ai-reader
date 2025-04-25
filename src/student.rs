use argon2::{
    Argon2, PasswordVerifier,
    password_hash::{PasswordHash, PasswordHasher, SaltString, rand_core::OsRng},
};

use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use utoipa::ToSchema;

use crate::{books::book::BookMeta, teacher::TeacherAgent};

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct StudentInfo {
    pub id: i64,
    pub name: String,
    pub email: String,
}

pub async fn get_student_list(database: &SqlitePool) -> anyhow::Result<Vec<StudentInfo>> {
    let students = sqlx::query_as!(StudentInfo, "SELECT id, name, email FROM student")
        .fetch_all(database)
        .await?;
    Ok(students)
}

pub async fn create_student(
    database: &SqlitePool,
    name: String,
    email: String,
    password: String,
) -> anyhow::Result<i64> {
    let salt = SaltString::generate(&mut OsRng);
    let password_hash = Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map_err(|e| anyhow::anyhow!("Failed to hash password: {}", e))?
        .to_string();
    let student = sqlx::query!(
        "INSERT INTO student (name, email, password) VALUES (?, ?, ?)",
        name,
        email,
        password_hash
    )
    .execute(database)
    .await?;
    Ok(student.last_insert_rowid() as i64)
}

pub async fn delete_student(database: &SqlitePool, id: i64) -> anyhow::Result<()> {
    sqlx::query!("DELETE FROM student WHERE id = ?", id)
        .execute(database)
        .await?;
    Ok(())
}

pub async fn get_student_books(database: &SqlitePool, id: i64) -> anyhow::Result<Vec<BookMeta>> {
    let books = sqlx::query!("SELECT book.id, book.title, book.authors, book.description, book.is_public FROM book inner join teacher_agent on book.id = teacher_agent.book_id WHERE student_id = ?", id)
        .fetch_all(database)
        .await?;
    let mut book_list = Vec::new();
    for book in books {
        let book_meta = BookMeta {
            id: book.id,
            title: book.title,
            authors: book.authors.split(',').map(|s| s.to_string()).collect(),
            description: book.description,
            is_public: book.is_public,
        };
        book_list.push(book_meta);
    }
    Ok(book_list)
}

pub async fn add_student_books(
    database: &SqlitePool,
    id: i64,
    book_ids: Vec<i64>,
) -> anyhow::Result<()> {
    for book_id in book_ids {
        TeacherAgent::init(id, book_id, database.clone()).await?;
    }
    Ok(())
}
pub async fn delete_student_book(
    database: &SqlitePool,
    id: i64,
    book_id: i64,
) -> anyhow::Result<()> {
    sqlx::query!(
        "DELETE FROM chapter_progress WHERE student_id = ? AND book_id = ?",
        id,
        book_id
    )
    .execute(database)
    .await?;
    sqlx::query!(
        "DELETE FROM history_message WHERE student_id = ? AND book_id = ?",
        id,
        book_id
    )
    .execute(database)
    .await?;
    sqlx::query!(
        "DELETE FROM teacher_agent WHERE student_id = ? AND book_id = ?",
        id,
        book_id
    )
    .execute(database)
    .await?;
    Ok(())
}

pub async fn login(database: &SqlitePool, email: String, password: String) -> anyhow::Result<i64> {
    let student = sqlx::query!("SELECT id, password FROM student WHERE email = ?", email)
        .fetch_one(database)
        .await?;
    let parsed_hash = PasswordHash::new(&student.password)
        .map_err(|e| anyhow::anyhow!("Failed to parse password hash: {}", e))?;
    Argon2::default()
        .verify_password(password.as_bytes(), &parsed_hash)
        .map_err(|e| anyhow::anyhow!("Failed to verify password: {}", e))?;
    Ok(student.id)
}

pub async fn get_student_info(database: &SqlitePool, id: i64) -> anyhow::Result<StudentInfo> {
    let student = sqlx::query_as!(
        StudentInfo,
        "SELECT id, name, email FROM student WHERE id = ?",
        id
    )
    .fetch_one(database)
    .await?;
    Ok(student)
}
