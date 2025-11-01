use anyhow::Result;
use tokio::sync::mpsc::UnboundedSender;
use tokio::task;

use crate::api::musicbrainz::{MusicBrainzClient, MusicBrainzError};
use crate::app::AppMessage;
use crate::library::LibraryStore;
use crate::models::{Album, AlbumRecord, Artist, CoverArtStatus};
use crate::notes::NoteService;
use crate::tasks::cover_art::CoverArtDownloaderHandle;

#[derive(Clone)]
pub struct AppController {
    client: MusicBrainzClient,
    library: LibraryStore,
    downloader: CoverArtDownloaderHandle,
    notes: NoteService,
    message_tx: UnboundedSender<AppMessage>,
}

impl AppController {
    pub fn new(
        client: MusicBrainzClient,
        library: LibraryStore,
        downloader: CoverArtDownloaderHandle,
        notes: NoteService,
        message_tx: UnboundedSender<AppMessage>,
    ) -> Self {
        Self {
            client,
            library,
            downloader,
            notes,
            message_tx,
        }
    }

    pub fn load_library(&self) -> Result<Vec<AlbumRecord>> {
        self.library.all_albums()
    }

    pub fn search_artists(&self, query: String) {
        if query.trim().is_empty() {
            return;
        }

        let client = self.client.clone();
        let tx = self.message_tx.clone();
        task::spawn(async move {
            match client.search_artists(&query).await {
                Ok(artists) => {
                    let _ = tx.send(AppMessage::ArtistResults(artists));
                }
                Err(MusicBrainzError::Empty) => {
                    let _ = tx.send(AppMessage::SearchFailed(format!(
                        "No artists found for '{query}'"
                    )));
                }
                Err(err) => {
                    let _ = tx.send(AppMessage::SearchFailed(format!(
                        "Artist search failed: {err}"
                    )));
                }
            }
        });
    }

    pub fn load_albums_for_artist(&self, artist: Artist) {
        let client = self.client.clone();
        let tx = self.message_tx.clone();
        let artist_id = artist.id.clone();
        let fallback_name = artist.display_name();

        task::spawn(async move {
            match client.albums_for_artist(&artist_id).await {
                Ok(mut albums) => {
                    for album in &mut albums {
                        if album.artist.is_empty() {
                            album.artist = fallback_name.clone();
                        }
                    }

                    let _ = tx.send(AppMessage::AlbumsLoaded(albums));
                }
                Err(MusicBrainzError::Empty) => {
                    let _ = tx.send(AppMessage::SearchFailed(format!(
                        "No albums found for {}",
                        fallback_name
                    )));
                }
                Err(err) => {
                    let _ = tx.send(AppMessage::SearchFailed(format!(
                        "Album fetch failed: {err}"
                    )));
                }
            }
        });
    }

    pub fn add_albums(&self, albums: Vec<Album>) -> Result<()> {
        if albums.is_empty() {
            return Ok(());
        }

        let mut added_any = false;

        for album in albums {
            let existing = self.library.get_album(&album.id)?;

            if existing.is_none() {
                // Add minimal record immediately
                let mut record = AlbumRecord::from_album(&album);
                record.cover_art_status = CoverArtStatus::Pending;
                self.library.upsert_album(record.clone())?;
                added_any = true;

                // Fetch full details in background
                let client = self.client.clone();
                let library = self.library.clone();
                let downloader = self.downloader.clone();
                let tx = self.message_tx.clone();
                let release_group_id = album.id.clone();

                task::spawn(async move {
                    let _ = tx.send(AppMessage::DownloadLog(format!(
                        "Fetching metadata for {}...",
                        record.title
                    )));

                    match client.fetch_album_details(&release_group_id).await {
                        Ok(full_album) => {
                            let mut full_record = AlbumRecord::from_album(&full_album);
                            full_record.cover_art_status = CoverArtStatus::Queued;

                            if let Err(err) = library.upsert_album(full_record.clone()) {
                                let _ = tx.send(AppMessage::DownloadLog(format!(
                                    "Failed to save metadata for {}: {err}",
                                    full_record.title
                                )));
                                return;
                            }

                            let _ = tx.send(AppMessage::DownloadLog(format!(
                                "Metadata fetched for {} - {}",
                                full_record.artist, full_record.title
                            )));

                            // Queue cover art download
                            if let Err(err) = downloader.enqueue(full_record.clone()) {
                                let _ = tx.send(AppMessage::DownloadLog(format!(
                                    "Failed to queue cover art for {}: {err}",
                                    full_record.title
                                )));
                            }

                            // Refresh library view
                            if let Ok(all) = library.all_albums() {
                                let _ = tx.send(AppMessage::LibraryRefreshed(all));
                            }
                        }
                        Err(err) => {
                            let _ = tx.send(AppMessage::DownloadLog(format!(
                                "Failed to fetch metadata for {}: {err}",
                                record.title
                            )));
                        }
                    }
                });
            }
        }

        if added_any {
            let all = self.library.all_albums()?;
            let _ = self.message_tx.send(AppMessage::LibraryRefreshed(all));
        }

        Ok(())
    }

    pub fn generate_notes(&self, records: Vec<AlbumRecord>) {
        if records.is_empty() {
            return;
        }

        let notes = self.notes.clone();
        let tx = self.message_tx.clone();

        task::spawn(async move {
            let outcome = task::spawn_blocking(move || notes.generate_notes(&records)).await;

            match outcome {
                Ok(Ok(logs)) => {
                    let _ = tx.send(AppMessage::NotesGenerated(logs));
                }
                Ok(Err(err)) => {
                    let _ = tx.send(AppMessage::DownloadLog(format!(
                        "Note generation failed: {err}"
                    )));
                }
                Err(join_err) => {
                    let _ = tx.send(AppMessage::DownloadLog(format!(
                        "Note generation task panicked: {join_err}"
                    )));
                }
            }
        });
    }

    pub fn add_album_by_release_id(&self, id: String) {
        let id = id.trim().to_string();
        
        if id.is_empty() {
            return;
        }

        // Validate UUID format (basic check)
        if id.len() != 36 || id.chars().filter(|c| *c == '-').count() != 4 {
            let _ = self.message_tx.send(AppMessage::DownloadLog(format!(
                "Invalid ID format: {} (expected UUID format like 'xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx')",
                id
            )));
            return;
        }

        let client = self.client.clone();
        let library = self.library.clone();
        let downloader = self.downloader.clone();
        let tx = self.message_tx.clone();

        task::spawn(async move {
            let _ = tx.send(AppMessage::DownloadLog(format!(
                "Fetching {}...",
                id
            )));

            // Try as release ID first
            match client.fetch_album_by_release_id(&id).await {
                Ok(album) => {
                    Self::process_fetched_album(album, library, downloader, tx).await;
                    return;
                }
                Err(err) => {
                    let err_str = err.to_string();
                    if err_str.contains("404") {
                        // Not found as release, try as release-group
                        let _ = tx.send(AppMessage::DownloadLog(format!(
                            "Not a release ID, trying as release-group ID..."
                        )));
                        
                        match client.fetch_album_details(&id).await {
                            Ok(album) => {
                                Self::process_fetched_album(album, library, downloader, tx).await;
                                return;
                            }
                            Err(rg_err) => {
                                let _ = tx.send(AppMessage::DownloadLog(format!(
                                    "ID not found as release or release-group: {} (check the ID is correct)",
                                    id
                                )));
                                return;
                            }
                        }
                    } else if err_str.contains("503") {
                        let _ = tx.send(AppMessage::DownloadLog(
                            "MusicBrainz service unavailable (rate limited). Wait a moment and try again.".to_string()
                        ));
                        return;
                    } else {
                        let _ = tx.send(AppMessage::DownloadLog(format!(
                            "Failed to fetch: {}",
                            err
                        )));
                        return;
                    }
                }
            }
        });
    }

    async fn process_fetched_album(
        album: crate::models::Album,
        library: LibraryStore,
        downloader: CoverArtDownloaderHandle,
        tx: UnboundedSender<AppMessage>,
    ) {
        match library.get_album(&album.id) {
            Ok(Some(existing)) => {
                // Album exists - update it with new release info if different
                let mut record = AlbumRecord::from_album(&album);
                
                // Preserve existing cover art and note status if already processed
                if existing.cover_art_status == CoverArtStatus::Completed {
                    record.cover_art_status = existing.cover_art_status;
                    record.cover_art_path = existing.cover_art_path;
                } else {
                    // Re-queue cover art download with new release ID
                    record.cover_art_status = CoverArtStatus::Queued;
                }
                
                record.note_path = existing.note_path;
                record.note_status = existing.note_status;

                if let Err(err) = library.upsert_album(record.clone()) {
                    let _ = tx.send(AppMessage::DownloadLog(format!(
                        "Failed to update album: {err}"
                    )));
                    return;
                }

                let _ = tx.send(AppMessage::DownloadLog(format!(
                    "Updated album in library: {} - {}",
                    record.artist, record.title
                )));

                // Re-queue cover art if it wasn't completed
                if existing.cover_art_status != CoverArtStatus::Completed {
                    if let Err(err) = downloader.enqueue(record.clone()) {
                        let _ = tx.send(AppMessage::DownloadLog(format!(
                            "Failed to queue cover art: {err}"
                        )));
                    }
                }

                // Refresh library view
                if let Ok(all) = library.all_albums() {
                    let _ = tx.send(AppMessage::LibraryRefreshed(all));
                }
            }
            Ok(None) => {
                // Add new album to library
                let mut record = AlbumRecord::from_album(&album);
                record.cover_art_status = CoverArtStatus::Queued;

                if let Err(err) = library.upsert_album(record.clone()) {
                    let _ = tx.send(AppMessage::DownloadLog(format!(
                        "Failed to save album: {err}"
                    )));
                    return;
                }

                let _ = tx.send(AppMessage::DownloadLog(format!(
                    "Added to library: {} - {}",
                    record.artist, record.title
                )));

                // Queue cover art download
                if let Err(err) = downloader.enqueue(record.clone()) {
                    let _ = tx.send(AppMessage::DownloadLog(format!(
                        "Failed to queue cover art: {err}"
                    )));
                }

                // Refresh library view
                if let Ok(all) = library.all_albums() {
                    let _ = tx.send(AppMessage::LibraryRefreshed(all));
                }
            }
            Err(err) => {
                let _ = tx.send(AppMessage::DownloadLog(format!(
                    "Database error: {err}"
                )));
            }
        }
    }
}
