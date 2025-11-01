use crate::models::{Album, AlbumRecord, Artist, CoverArtStatus};

#[derive(Debug, Clone)]
pub enum AppMessage {
    ArtistResults(Vec<Artist>),
    AlbumsLoaded(Vec<Album>),
    SearchFailed(String),
    CoverArtStatus {
        mbid: String,
        status: CoverArtStatus,
        path: Option<String>,
    },
    DownloadLog(String),
    LibraryRefreshed(Vec<AlbumRecord>),
    NotesGenerated(Vec<String>),
}
