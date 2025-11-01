use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct TrackInfo {
    pub position: String,
    pub title: String,
    pub length_ms: i64,
}

impl Default for TrackInfo {
    fn default() -> Self {
        Self {
            position: String::new(),
            title: String::new(),
            length_ms: 0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Album {
    pub id: String,
    pub release_id: String, // Actual release ID for cover art
    pub title: String,
    pub artist: String,
    pub primary_type: String,
    pub secondary_types: Vec<String>,
    pub status: String,
    pub first_release_date: String,
    pub disambiguation: String,
    pub label: String,
    pub country: String,
    pub tracklist: Vec<TrackInfo>,
}

impl Default for Album {
    fn default() -> Self {
        Self {
            id: String::new(),
            release_id: String::new(),
            title: String::new(),
            artist: String::new(),
            primary_type: String::new(),
            secondary_types: Vec::new(),
            status: String::new(),
            first_release_date: String::new(),
            disambiguation: String::new(),
            label: String::new(),
            country: String::new(),
            tracklist: Vec::new(),
        }
    }
}

impl Album {
    pub fn cover_art_url(&self) -> String {
        let id_for_cover = if self.release_id.is_empty() {
            &self.id
        } else {
            &self.release_id
        };
        format!("https://coverartarchive.org/release/{}/front", id_for_cover)
    }

    pub fn secondary_types_label(&self) -> String {
        if self.secondary_types.is_empty() {
            "None".to_string()
        } else {
            self.secondary_types.join(", ")
        }
    }
}
