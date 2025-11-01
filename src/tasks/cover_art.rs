use std::path::PathBuf;
use std::time::Duration;

use anyhow::{Context, Result};
use reqwest::{Client, header};
use tokio::sync::mpsc::{self, UnboundedReceiver, UnboundedSender};
use tokio::time::{MissedTickBehavior, interval};

use crate::app::AppMessage;
use crate::config::AppConfig;
use crate::library::LibraryStore;
use crate::models::{AlbumRecord, CoverArtStatus};

#[derive(Clone)]
pub struct CoverArtDownloaderHandle {
    tx: UnboundedSender<CoverArtJob>,
}

impl CoverArtDownloaderHandle {
    pub fn enqueue(&self, record: AlbumRecord) -> Result<()> {
        self.tx
            .send(CoverArtJob { record })
            .context("failed to enqueue cover art job")
    }
}

pub fn spawn(
    config: AppConfig,
    library: LibraryStore,
    message_tx: UnboundedSender<AppMessage>,
) -> Result<CoverArtDownloaderHandle> {
    let (tx, rx) = mpsc::unbounded_channel();

    let client = build_client(&config)?;
    let album_art_dir = PathBuf::from(config.album_art_dir());

    tokio::spawn(async move {
        run_downloader(client, library, message_tx, album_art_dir, rx).await;
    });

    Ok(CoverArtDownloaderHandle { tx })
}

struct CoverArtJob {
    record: AlbumRecord,
}

fn build_client(config: &AppConfig) -> Result<Client> {
    let mut headers = header::HeaderMap::new();
    headers.insert(
        header::USER_AGENT,
        header::HeaderValue::from_str(config.user_agent())
            .context("invalid user agent header value for cover art client")?,
    );
    headers.insert(
        "X-Client-Id",
        header::HeaderValue::from_str(config.client_id())
            .context("invalid client identifier for cover art client")?,
    );
    headers.insert(
        header::ACCEPT,
        header::HeaderValue::from_static("image/jpeg, image/png, application/json"),
    );

    let client = Client::builder()
        .default_headers(headers)
        .build()
        .context("unable to create HTTP client for cover art downloads")?;

    Ok(client)
}

async fn run_downloader(
    client: Client,
    library: LibraryStore,
    message_tx: UnboundedSender<AppMessage>,
    album_art_dir: PathBuf,
    mut rx: UnboundedReceiver<CoverArtJob>,
) {
    let mut throttle = interval(Duration::from_secs(1));
    throttle.set_missed_tick_behavior(MissedTickBehavior::Delay);

    while let Some(job) = rx.recv().await {
        let record = job.record;
        let mbid = record.mbid.clone();

        let _ = message_tx.send(AppMessage::CoverArtStatus {
            mbid: mbid.clone(),
            status: CoverArtStatus::Queued,
            path: None,
        });

        throttle.tick().await;

        let download_result = download_cover_art(&client, &record, &album_art_dir).await;

        match download_result {
            Ok(path) => {
                if let Err(err) = library.set_cover_art_path(
                    &mbid,
                    Some(path.to_string_lossy().to_string()),
                    CoverArtStatus::Completed,
                ) {
                    let _ = message_tx.send(AppMessage::DownloadLog(format!(
                        "Failed to update library for {mbid}: {err}"
                    )));
                }

                let _ = message_tx.send(AppMessage::CoverArtStatus {
                    mbid,
                    status: CoverArtStatus::Completed,
                    path: Some(path.to_string_lossy().to_string()),
                });
            }
            Err(err) => {
                let _ = library.set_cover_art_path(&mbid, None, CoverArtStatus::Unavailable);
                let _ = message_tx.send(AppMessage::DownloadLog(format!(
                    "Cover art unavailable for {mbid}: {err}"
                )));
                let _ = message_tx.send(AppMessage::CoverArtStatus {
                    mbid,
                    status: CoverArtStatus::Unavailable,
                    path: None,
                });
            }
        }
    }
}

async fn download_cover_art(
    client: &Client,
    record: &AlbumRecord,
    album_art_dir: &PathBuf,
) -> Result<PathBuf> {
    let url = &record.cover_art_url;
    let response = client
        .get(url)
        .send()
        .await
        .context("failed to request cover art")?;

    if response.status().is_success() {
        let bytes = response
            .bytes()
            .await
            .context("failed to read cover art bytes")?;
        tokio::fs::create_dir_all(album_art_dir)
            .await
            .context("failed to ensure album art directory exists")?;
        let path = album_art_dir.join(record.cover_art_filename());
        tokio::fs::write(&path, &bytes)
            .await
            .context("failed to write cover art to disk")?;
        Ok(path)
    } else {
        Err(anyhow::anyhow!(
            "cover art returned status {}",
            response.status()
        ))
    }
}
