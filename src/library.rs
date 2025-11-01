use anyhow::{Context, Result};
use sled::IVec;

use crate::config::AppConfig;
use crate::models::library::{AlbumRecord, CoverArtStatus, NoteStatus};

#[derive(Clone)]
pub struct LibraryStore {
    tree: sled::Tree,
}

impl LibraryStore {
    pub fn open(config: &AppConfig) -> Result<Self> {
        let db = sled::open(config.db_path()).with_context(|| {
            format!(
                "Failed to open library database at {}",
                config.db_path().display()
            )
        })?;
        let tree = db
            .open_tree("albums")
            .context("Unable to open albums tree")?;
        Ok(Self { tree })
    }

    pub fn upsert_album(&self, mut record: AlbumRecord) -> Result<bool> {
        record.touch();
        let key = Self::album_key(&record.mbid);
        let value = serde_json::to_vec(&record).context("Failed to serialize album record")?;

        let is_new = self.tree.get(&key)?.is_none();
        self.tree
            .insert(key, value)
            .context("Failed to persist album record")?;
        self.tree.flush()?;
        Ok(is_new)
    }

    pub fn get_album(&self, mbid: &str) -> Result<Option<AlbumRecord>> {
        self.tree
            .get(Self::album_key(mbid))?
            .map(|bytes| Self::deserialize_record(bytes))
            .transpose()
    }

    pub fn all_albums(&self) -> Result<Vec<AlbumRecord>> {
        let mut records = Vec::new();
        for result in self.tree.iter() {
            let (_, value) = result?;
            if let Ok(record) = Self::deserialize_record(value) {
                records.push(record);
            }
        }
        records.sort_by(|a, b| a.title.to_lowercase().cmp(&b.title.to_lowercase()));
        Ok(records)
    }

    pub fn set_cover_art_path(
        &self,
        mbid: &str,
        path: Option<String>,
        status: CoverArtStatus,
    ) -> Result<()> {
        if let Some(mut record) = self.get_album(mbid)? {
            record.cover_art_path = path;
            record.cover_art_status = status;
            record.touch();
            self.upsert_album(record)?;
        }
        Ok(())
    }

    pub fn mark_note_generated(&self, mbid: &str, note_path: String) -> Result<()> {
        if let Some(mut record) = self.get_album(mbid)? {
            record.note_status = NoteStatus::Generated;
            record.note_path = Some(note_path);
            record.touch();
            self.upsert_album(record)?;
        }
        Ok(())
    }

    fn deserialize_record(bytes: IVec) -> Result<AlbumRecord> {
        serde_json::from_slice::<AlbumRecord>(&bytes).context("Unable to deserialize album record")
    }

    fn album_key(id: &str) -> Vec<u8> {
        format!("album::{id}").into_bytes()
    }
}
