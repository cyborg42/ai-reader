pub mod manager;
pub mod public;
pub mod user;

use std::sync::Arc;

use actix_multipart::Multipart;
use anyhow::bail;
use futures::TryStreamExt;
use tokio::{fs::File, io::AsyncWriteExt};

use crate::books::library::Library;

pub async fn upload_books(
    mut payload: Multipart,
    library: Arc<Library>,
) -> anyhow::Result<Vec<i64>> {
    let mut book_ids = Vec::new();
    while let Some(mut field) = payload
        .try_next()
        .await
        .map_err(|e| anyhow::anyhow!("Failed to read chunk: {}", e))?
    {
        let Some(filename) = field.content_disposition().and_then(|s| s.get_filename()) else {
            bail!("No filename found");
        };
        let temp_dir = tempfile::tempdir()?;
        let path = temp_dir.path().join(filename);
        let mut file = File::create(&path).await?;
        while let Some(chunk) = field
            .try_next()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to read chunk: {}", e))?
        {
            file.write_all(&chunk).await?;
        }
        let book_id = library.upload_book(path).await?;
        book_ids.push(book_id);
    }
    Ok(book_ids)
}
