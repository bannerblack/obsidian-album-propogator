use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use uuid::Uuid;

/// Static configuration and filesystem paths used throughout the application.
#[derive(Clone, Debug)]
pub struct AppConfig {
    data_dir: PathBuf,
    album_art_dir: PathBuf,
    notes_dir: PathBuf,
    db_path: PathBuf,
    template_path: PathBuf,
    user_agent: String,
    client_id: String,
}

impl Default for AppConfig {
    fn default() -> Self {
        let base = PathBuf::from("data");
        let album_art = base.join("album_art");
        let notes = base.join("notes");
        let db_path = base.join("library.db");
        let templates = PathBuf::from("templates");

        let client_id = format!("rust-mb-client-{}", Uuid::new_v4());
        let user_agent =
            format!("rust-mb-library/0.1.0 ( https://musicbrainz.org ; unique-id={client_id} )");

        Self {
            data_dir: base,
            album_art_dir: album_art,
            notes_dir: notes,
            db_path,
            template_path: templates.join("note_template.md"),
            user_agent,
            client_id,
        }
    }
}

impl AppConfig {
    pub fn data_dir(&self) -> &Path {
        &self.data_dir
    }

    pub fn album_art_dir(&self) -> &Path {
        &self.album_art_dir
    }

    pub fn notes_dir(&self) -> &Path {
        &self.notes_dir
    }

    pub fn db_path(&self) -> &Path {
        &self.db_path
    }

    pub fn template_path(&self) -> &Path {
        &self.template_path
    }

    pub fn user_agent(&self) -> &str {
        &self.user_agent
    }

    pub fn client_id(&self) -> &str {
        &self.client_id
    }

    /// Ensures that required directories exist and bootstraps default template content.
    pub fn ensure_filesystem(&self) -> Result<()> {
        for path in [
            self.data_dir(),
            self.album_art_dir(),
            self.notes_dir(),
            self.template_path()
                .parent()
                .unwrap_or_else(|| Path::new("templates")),
        ] {
            fs::create_dir_all(path)
                .with_context(|| format!("Failed to create directory: {}", path.display()))?;
        }

        if !self.template_path().exists() {
            let default_template = r#"---
title: {title}
artist: {artist}
release_date: {release_date}
musicbrainz_id: {musicbrainz_id}
primary_type: {primary_type}
---

# {title}

**Artist:** {artist}

**Release Date:** {release_date}

**Primary Type:** {primary_type}

**Secondary Types:** {secondary_types}

**Cover Art:** ![]({cover_art_relative_path})

## Tracklist

{track_listing}

## Notes

- 

"#;

            fs::write(self.template_path(), default_template).with_context(|| {
                format!(
                    "Failed to write default note template to {}",
                    self.template_path().display()
                )
            })?;
        }

        Ok(())
    }
}
