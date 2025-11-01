use chrono::Utc;
use serde::{Deserialize, Serialize};

use super::album::{Album, TrackInfo};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CoverArtStatus {
    Pending,
    Queued,
    Downloading,
    Completed,
    Unavailable,
}

impl Default for CoverArtStatus {
    fn default() -> Self {
        Self::Pending
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum NoteStatus {
    NotGenerated,
    Generated,
}

impl Default for NoteStatus {
    fn default() -> Self {
        Self::NotGenerated
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AlbumRecord {
    pub mbid: String,
    pub title: String,
    pub artist: String,
    pub primary_type: String,
    pub secondary_types: Vec<String>,
    pub status: String,
    pub release_date: String,
    pub label: String,
    pub country: String,
    pub disambiguation: String,
    pub cover_art_url: String,
    pub cover_art_path: Option<String>,
    pub note_path: Option<String>,
    pub tracklist: Vec<TrackInfo>,
    pub cover_art_status: CoverArtStatus,
    pub note_status: NoteStatus,
    pub created_at_utc: String,
    pub updated_at_utc: String,
}

impl Default for AlbumRecord {
    fn default() -> Self {
        let now = Utc::now();
        Self {
            mbid: String::new(),
            title: String::new(),
            artist: String::new(),
            primary_type: String::new(),
            secondary_types: Vec::new(),
            status: String::new(),
            release_date: String::new(),
            label: String::new(),
            country: String::new(),
            disambiguation: String::new(),
            cover_art_url: String::new(),
            cover_art_path: None,
            note_path: None,
            tracklist: Vec::new(),
            cover_art_status: CoverArtStatus::Pending,
            note_status: NoteStatus::NotGenerated,
            created_at_utc: now.to_rfc3339(),
            updated_at_utc: now.to_rfc3339(),
        }
    }
}

impl AlbumRecord {
    pub fn from_album(album: &Album) -> Self {
        let now = Utc::now().to_rfc3339();
        Self {
            mbid: album.id.clone(),
            title: album.title.clone(),
            artist: album.artist.clone(),
            primary_type: album.primary_type.clone(),
            secondary_types: album.secondary_types.clone(),
            status: album.status.clone(),
            release_date: album.first_release_date.clone(),
            label: album.label.clone(),
            country: album.country.clone(),
            disambiguation: album.disambiguation.clone(),
            cover_art_url: album.cover_art_url(),
            cover_art_path: None,
            note_path: None,
            tracklist: album.tracklist.clone(),
            cover_art_status: CoverArtStatus::Pending,
            note_status: NoteStatus::NotGenerated,
            created_at_utc: now.clone(),
            updated_at_utc: now,
        }
    }

    pub fn touch(&mut self) {
        self.updated_at_utc = Utc::now().to_rfc3339();
    }

    pub fn cover_art_filename(&self) -> String {
        format!("{}.jpg", self.mbid)
    }

    pub fn note_filename(&self) -> String {
        format!("{} - {}.md", self.artist, self.title)
    }

    pub fn as_track_listing_lines(&self) -> Vec<String> {
        self.tracklist
            .iter()
            .enumerate()
            .map(|(idx, track)| {
                let index = idx + 1;
                if track.length_ms <= 0 {
                    format!("{index}. {}", track.title)
                } else {
                    let minutes = track.length_ms / 60000;
                    let seconds = (track.length_ms % 60000) / 1000;
                    format!("{index}. {} ({minutes:02}:{seconds:02})", track.title)
                }
            })
            .collect()
    }

    pub fn secondary_types_label(&self) -> String {
        if self.secondary_types.is_empty() {
            "None".to_string()
        } else {
            self.secondary_types.join(", ")
        }
    }
}
