mod api;
mod app;
mod config;
mod library;
mod models;
mod notes;
mod tasks;
mod tui;

use anyhow::Result;

#[tokio::main]
async fn main() -> Result<()> {
    let config = config::AppConfig::default();
    config.ensure_filesystem()?;

    let (msg_tx, msg_rx) = tokio::sync::mpsc::unbounded_channel();

    let client = api::musicbrainz::MusicBrainzClient::new(&config)?;
    let library = library::LibraryStore::open(&config)?;
    let downloader = tasks::cover_art::spawn(config.clone(), library.clone(), msg_tx.clone())?;
    let note_service = notes::NoteService::new(config.clone(), library.clone());

    let controller = tui::AppController::new(client, library, downloader, note_service, msg_tx);

    let app = tui::App::new(controller, msg_rx);
    tui::run(app).await
}
