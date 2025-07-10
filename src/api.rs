pub mod manager;
pub mod public;
pub mod user;

use std::sync::Arc;

use axum::extract::Multipart;
use tokio::{fs::File, io::AsyncWriteExt};

use crate::books::library::Library;

pub async fn upload_books(
    mut multipart: Multipart,
    library: Arc<Library>,
) -> anyhow::Result<Vec<i64>> {
    let mut book_ids = Vec::new();
    while let Some(mut field) = multipart.next_field().await? {
        let filename = field
            .file_name()
            .ok_or_else(|| anyhow::anyhow!("No filename found"))?
            .to_string();
        let temp_dir = tempfile::tempdir()?;
        let path = temp_dir.path().join(filename);
        let mut file = File::create(&path).await?;
        while let Some(chunk) = field.chunk().await? {
            file.write_all(&chunk).await?;
        }
        let book_id = library.upload_book(path).await?;
        book_ids.push(book_id);
    }
    Ok(book_ids)
}

