use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use crate::config::AppConfig;
use crate::library::LibraryStore;
use crate::models::AlbumRecord;

#[derive(Clone)]
pub struct NoteService {
    config: AppConfig,
    library: LibraryStore,
}

impl NoteService {
    pub fn new(config: AppConfig, library: LibraryStore) -> Self {
        Self { config, library }
    }

    pub fn generate_notes(&self, albums: &[AlbumRecord]) -> Result<Vec<String>> {
        let template = fs::read_to_string(self.config.template_path()).with_context(|| {
            format!(
                "Unable to read note template at {}",
                self.config.template_path().display()
            )
        })?;

        let mut logs = Vec::new();

        for album in albums {
            // Skip if artist or title is empty (metadata not yet fetched)
            if album.artist.is_empty() || album.title.is_empty() {
                logs.push(format!(
                    "Skipped {} - metadata not yet loaded",
                    if album.title.is_empty() {
                        &album.mbid
                    } else {
                        &album.title
                    }
                ));
                continue;
            }

            let filename = sanitize_filename::sanitize(album.note_filename());
            let path = Path::new(self.config.notes_dir()).join(&filename);

            if path.exists() {
                logs.push(format!(
                    "Skipped existing note for {} - {}",
                    album.artist, album.title
                ));
                continue;
            }

            // Wait for cover art path to be set (either downloaded or marked unavailable)
            let cover_art_relative = if let Some(art_path) = &album.cover_art_path {
                pathdiff::diff_paths(Path::new(art_path), self.config.notes_dir())
                    .unwrap_or_else(|| PathBuf::from(art_path))
            } else {
                // Cover art not yet processed, skip for now
                logs.push(format!(
                    "Skipped {} - {} (waiting for cover art)",
                    album.artist, album.title
                ));
                continue;
            };

            let body = render_template(
                &template,
                album,
                cover_art_relative
                    .to_string_lossy()
                    .replace('\r', "")
                    .replace('\n', "/"),
            );

            fs::write(&path, body).with_context(|| {
                format!(
                    "Unable to write note for {} - {}",
                    album.artist, album.title
                )
            })?;

            self.library
                .mark_note_generated(&album.mbid, path.to_string_lossy().to_string())?;

            logs.push(format!("Generated note: {}", path.to_string_lossy()));
        }

        Ok(logs)
    }
}

fn render_template(template: &str, album: &AlbumRecord, cover_art_path: String) -> String {
    let mut body = template.to_string();
    body = body.replace("{title}", &album.title);
    body = body.replace("{artist}", &album.artist);
    body = body.replace("{release_date}", &album.release_date);
    body = body.replace("{musicbrainz_id}", &album.mbid);
    body = body.replace("{primary_type}", &album.primary_type);
    body = body.replace("{secondary_types}", &album.secondary_types_label());
    body = body.replace("{cover_art_relative_path}", &cover_art_path);

    let track_listing = if album.tracklist.is_empty() {
        String::from("- Track details unavailable")
    } else {
        album.as_track_listing_lines().join("\n")
    };

    body = body.replace("{track_listing}", &track_listing);

    body
}
