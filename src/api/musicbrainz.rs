use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use chrono::NaiveDate;
use reqwest::{Client, Url, header};
use serde::Deserialize;
use thiserror::Error;
use tokio::sync::Mutex;
use tokio::time::sleep;

use crate::config::AppConfig;
use crate::models::album::{Album, TrackInfo};
use crate::models::artist::Artist;

#[derive(Debug, Error)]
pub enum MusicBrainzError {
    #[error("network error: {0}")]
    Network(#[from] reqwest::Error),
    #[error("failed to parse response: {0}")]
    Parse(String),
    #[error("no results returned")]
    Empty,
}

#[derive(Clone)]
pub struct MusicBrainzClient {
    http: Client,
    base_headers: header::HeaderMap,
    throttle: Arc<Mutex<Option<Instant>>>,
}

impl MusicBrainzClient {
    pub fn new(config: &AppConfig) -> Result<Self> {
        let mut headers = header::HeaderMap::new();
        headers.insert(
            header::USER_AGENT,
            header::HeaderValue::from_str(config.user_agent())
                .context("invalid user agent header value")?,
        );
        headers.insert(
            header::ACCEPT,
            header::HeaderValue::from_static("application/json"),
        );
        headers.insert(
            "X-Client-Id",
            header::HeaderValue::from_str(config.client_id())
                .context("invalid client identifier header value")?,
        );

        let http = Client::builder()
            .default_headers(headers.clone())
            .user_agent(config.user_agent())
            .timeout(Duration::from_secs(30))
            .connect_timeout(Duration::from_secs(10))
            .build()
            .context("unable to construct http client")?;

        Ok(Self {
            http,
            base_headers: headers,
            throttle: Arc::new(Mutex::new(None)),
        })
    }

    pub async fn search_artists(&self, query: &str) -> Result<Vec<Artist>, MusicBrainzError> {
        let url = Url::parse_with_params(
            "https://musicbrainz.org/ws/2/artist",
            [("query", query), ("fmt", "json"), ("limit", "25")],
        )
        .map_err(|err| MusicBrainzError::Parse(err.to_string()))?;

        self.await_throttle().await;
        let response = self
            .http
            .get(url)
            .headers(self.base_headers.clone())
            .send()
            .await?
            .error_for_status()?;

        let body: ArtistSearchResponse = response
            .json()
            .await
            .map_err(|err| MusicBrainzError::Parse(err.to_string()))?;

        let artists = body
            .artists
            .into_iter()
            .map(|item| Artist {
                id: item.id,
                name: item.name,
                disambiguation: item.disambiguation,
                score: item.score,
            })
            .collect::<Vec<_>>();

        if artists.is_empty() {
            return Err(MusicBrainzError::Empty);
        }

        Ok(artists)
    }

    pub async fn albums_for_artist(&self, artist_id: &str) -> Result<Vec<Album>, MusicBrainzError> {
        const PAGE_SIZE: usize = 100;
        let mut albums: Vec<Album> = Vec::new();
        let mut offset: usize = 0;

        // Fetch all album/EP release groups (fast, minimal data)
        loop {
            let limit = PAGE_SIZE.to_string();
            let offset_str = offset.to_string();
            let url = Url::parse_with_params(
                "https://musicbrainz.org/ws/2/release-group",
                [
                    ("artist", artist_id),
                    ("fmt", "json"),
                    ("limit", limit.as_str()),
                    ("offset", offset_str.as_str()),
                    ("type", "album|ep"),
                ],
            )
            .map_err(|err| MusicBrainzError::Parse(err.to_string()))?;

            self.await_throttle().await;
            let response = self
                .http
                .get(url)
                .headers(self.base_headers.clone())
                .send()
                .await?
                .error_for_status()?;

            let body: ReleaseGroupResponse = response
                .json()
                .await
                .map_err(|err| MusicBrainzError::Parse(err.to_string()))?;

            if body.release_groups.is_empty() {
                break;
            }

            let batch_len = body.release_groups.len();
            
            // Convert release groups to minimal Album structs
            for group in body.release_groups {
                let album = Album {
                    id: group.id,
                    release_id: String::new(), // Will be filled in when metadata is fetched
                    title: group.title,
                    artist: String::new(), // Will be filled in when added to library
                    primary_type: group.primary_type.unwrap_or_default(),
                    secondary_types: group.secondary_types.unwrap_or_default(),
                    status: String::new(),
                    first_release_date: group.first_release_date.unwrap_or_default(),
                    disambiguation: group.disambiguation,
                    label: String::new(),
                    country: String::new(),
                    tracklist: Vec::new(),
                };
                albums.push(album);
            }

            offset += batch_len;
            if batch_len < PAGE_SIZE {
                break;
            }
        }

        if albums.is_empty() {
            return Err(MusicBrainzError::Empty);
        }

        albums.sort_by(|a, b| {
            parse_date(&a.first_release_date)
                .cmp(&parse_date(&b.first_release_date))
                .then_with(|| a.first_release_date.cmp(&b.first_release_date))
                .then_with(|| a.title.cmp(&b.title))
        });

        Ok(albums)
    }

    /// Fetch full metadata for a release group (used when adding to library)
    pub async fn fetch_album_details(&self, release_group_id: &str) -> Result<Album, MusicBrainzError> {
        // First, get the release group info
        let url = Url::parse_with_params(
            &format!("https://musicbrainz.org/ws/2/release-group/{release_group_id}"),
            [("fmt", "json"), ("inc", "artist-credits")],
        )
        .map_err(|err| MusicBrainzError::Parse(err.to_string()))?;

        self.await_throttle().await;
        let response = self
            .http
            .get(url)
            .headers(self.base_headers.clone())
            .send()
            .await?
            .error_for_status()?;

        let group: ReleaseGroupDetail = response
            .json()
            .await
            .map_err(|err| MusicBrainzError::Parse(err.to_string()))?;

        // Now fetch releases for this group
        let url = Url::parse_with_params(
            "https://musicbrainz.org/ws/2/release",
            [
                ("release-group", release_group_id),
                ("fmt", "json"),
                ("limit", "100"),
                ("status", "official"),
                ("inc", "recordings+labels"),
            ],
        )
        .map_err(|err| MusicBrainzError::Parse(err.to_string()))?;

        self.await_throttle().await;
        let response = self
            .http
            .get(url)
            .headers(self.base_headers.clone())
            .send()
            .await?
            .error_for_status()?;

        let body: ReleaseSearchResponse = response
            .json()
            .await
            .map_err(|err| MusicBrainzError::Parse(err.to_string()))?;

        if body.releases.is_empty() {
            return Err(MusicBrainzError::Empty);
        }

        // Find the earliest release
        let mut best_release: Option<(ReleaseItem, Option<NaiveDate>, String)> = None;
        let group_first_date = group.first_release_date.clone().unwrap_or_default();

        for release in body.releases {
            let release_date_raw = release.date.clone().unwrap_or_default();
            let effective_date = if release_date_raw.is_empty() {
                group_first_date.clone()
            } else {
                release_date_raw
            };
            let sort_date = parse_date(&effective_date).or_else(|| parse_date(&group_first_date));

            let should_use = match &best_release {
                None => true,
                Some((_, existing_sort, existing_value)) => {
                    should_replace_release(sort_date, &effective_date, *existing_sort, existing_value)
                }
            };

            if should_use {
                best_release = Some((release, sort_date, effective_date));
            }
        }

        let (release, _, _) = best_release.ok_or(MusicBrainzError::Empty)?;

        let album = Album {
            id: group.id.clone(), // Use release-group ID as primary ID
            release_id: release.id.clone(), // Store actual release ID for cover art
            title: group.title,
            artist: group
                .artist_credit
                .first()
                .map(|credit| credit.artist.name.clone())
                .unwrap_or_default(),
            primary_type: group.primary_type.unwrap_or_default(),
            secondary_types: group.secondary_types.unwrap_or_default(),
            status: release.status.unwrap_or_default(),
            first_release_date: group_first_date,
            disambiguation: group.disambiguation,
            label: release
                .label_info
                .iter()
                .find_map(|info| info.label.as_ref().and_then(|label| label.name.clone()))
                .unwrap_or_default(),
            country: release.country.unwrap_or_default(),
            tracklist: release
                .media
                .into_iter()
                .flat_map(|medium| medium.tracks.into_iter().map(TrackInfo::from))
                .collect::<Vec<_>>(),
        };

        Ok(album)
    }

    /// Fetch album details by release ID (for manual add)
    pub async fn fetch_album_by_release_id(&self, release_id: &str) -> Result<Album, MusicBrainzError> {
        // Fetch the release with full details - retry up to 3 times on network errors
        let url = Url::parse_with_params(
            &format!("https://musicbrainz.org/ws/2/release/{release_id}"),
            [("fmt", "json"), ("inc", "recordings+labels+release-groups+artist-credits")],
        )
        .map_err(|err| MusicBrainzError::Parse(err.to_string()))?;

        let mut last_error = None;
        for attempt in 1..=3 {
            self.await_throttle().await;
            
            match self
                .http
                .get(url.clone())
                .headers(self.base_headers.clone())
                .send()
                .await
            {
                Ok(response) => {
                    match response.error_for_status() {
                        Ok(resp) => {
                            let release: ReleaseItem = resp
                                .json()
                                .await
                                .map_err(|err| MusicBrainzError::Parse(err.to_string()))?;
                            
                            return self.build_album_from_release(release);
                        }
                        Err(e) => {
                            return Err(e.into());
                        }
                    }
                }
                Err(e) => {
                    last_error = Some(e);
                    if attempt < 3 {
                        // Wait before retry (exponential backoff)
                        sleep(Duration::from_millis(1000 * attempt)).await;
                    }
                }
            }
        }

        Err(last_error.unwrap().into())
    }

    fn build_album_from_release(&self, release: ReleaseItem) -> Result<Album, MusicBrainzError> {
        let album = Album {
            id: release.release_group.id.clone(), // Use release-group ID as primary ID
            release_id: release.id.clone(), // Store actual release ID for cover art
            title: release.title,
            artist: release
                .artist_credit
                .first()
                .map(|credit| credit.artist.name.clone())
                .unwrap_or_default(),
            primary_type: release.release_group.primary_type.unwrap_or_default(),
            secondary_types: release.release_group.secondary_types.unwrap_or_default(),
            status: release.status.unwrap_or_default(),
            first_release_date: release.release_group.first_release_date.unwrap_or_default(),
            disambiguation: release.disambiguation,
            label: release
                .label_info
                .iter()
                .find_map(|info| info.label.as_ref().and_then(|label| label.name.clone()))
                .unwrap_or_default(),
            country: release.country.unwrap_or_default(),
            tracklist: release
                .media
                .into_iter()
                .flat_map(|medium| medium.tracks.into_iter().map(TrackInfo::from))
                .collect::<Vec<_>>(),
        };

        Ok(album)
    }

    async fn await_throttle(&self) {
        let mut guard = self.throttle.lock().await;
        if let Some(last) = *guard {
            let elapsed = last.elapsed();
            if elapsed < Duration::from_millis(1100) {
                sleep(Duration::from_millis(1100) - elapsed).await;
            }
        }
        *guard = Some(Instant::now());
    }
}

fn should_replace_release(
    new_sort: Option<NaiveDate>,
    new_value: &str,
    existing_sort: Option<NaiveDate>,
    existing_value: &str,
) -> bool {
    match (existing_sort, new_sort) {
        (Some(old), Some(new)) => {
            if new < old {
                true
            } else if new > old {
                false
            } else {
                new_value < existing_value
            }
        }
        (None, Some(_)) => true,
        (Some(_), None) => false,
        (None, None) => match (existing_value.is_empty(), new_value.is_empty()) {
            (true, true) => false,
            (true, false) => true,
            (false, true) => false,
            (false, false) => new_value < existing_value,
        },
    }
}

fn parse_date(value: &str) -> Option<NaiveDate> {
    if value.is_empty() {
        return None;
    }

    if let Ok(date) = NaiveDate::parse_from_str(value, "%Y-%m-%d") {
        return Some(date);
    }

    if value.len() == 7 {
        let padded = format!("{value}-01");
        return NaiveDate::parse_from_str(&padded, "%Y-%m-%d").ok();
    }

    if value.len() == 4 {
        if let Ok(year) = value.parse::<i32>() {
            return NaiveDate::from_ymd_opt(year, 1, 1);
        }
    }

    None
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
struct ArtistSearchResponse {
    artists: Vec<ArtistItem>,
}

impl Default for ArtistSearchResponse {
    fn default() -> Self {
        Self {
            artists: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
struct ReleaseSearchResponse {
    releases: Vec<ReleaseItem>,
}

impl Default for ReleaseSearchResponse {
    fn default() -> Self {
        Self {
            releases: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
struct ReleaseGroupResponse {
    #[serde(rename = "release-groups")]
    release_groups: Vec<ReleaseGroupItem>,
}

impl Default for ReleaseGroupResponse {
    fn default() -> Self {
        Self {
            release_groups: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
struct ReleaseGroupItem {
    id: String,
    title: String,
    disambiguation: String,
    #[serde(rename = "primary-type")]
    primary_type: Option<String>,
    #[serde(rename = "secondary-types")]
    secondary_types: Option<Vec<String>>,
    #[serde(rename = "first-release-date")]
    first_release_date: Option<String>,
}

impl Default for ReleaseGroupItem {
    fn default() -> Self {
        Self {
            id: String::new(),
            title: String::new(),
            disambiguation: String::new(),
            primary_type: None,
            secondary_types: Some(Vec::new()),
            first_release_date: None,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
struct ReleaseGroupDetail {
    id: String,
    title: String,
    disambiguation: String,
    #[serde(rename = "primary-type")]
    primary_type: Option<String>,
    #[serde(rename = "secondary-types")]
    secondary_types: Option<Vec<String>>,
    #[serde(rename = "first-release-date")]
    first_release_date: Option<String>,
    #[serde(rename = "artist-credit")]
    artist_credit: Vec<ArtistCredit>,
}

impl Default for ReleaseGroupDetail {
    fn default() -> Self {
        Self {
            id: String::new(),
            title: String::new(),
            disambiguation: String::new(),
            primary_type: None,
            secondary_types: Some(Vec::new()),
            first_release_date: None,
            artist_credit: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
struct ArtistItem {
    id: String,
    name: String,
    disambiguation: String,
    score: i32,
}

impl Default for ArtistItem {
    fn default() -> Self {
        Self {
            id: String::new(),
            name: String::new(),
            disambiguation: String::new(),
            score: 0,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
struct ReleaseItem {
    id: String,
    title: String,
    #[serde(default)]
    status: Option<String>,
    #[serde(default)]
    country: Option<String>,
    #[serde(default)]
    date: Option<String>,
    disambiguation: String,
    #[serde(rename = "release-group")]
    release_group: ReleaseGroup,
    #[serde(rename = "artist-credit")]
    artist_credit: Vec<ArtistCredit>,
    #[serde(rename = "label-info", default)]
    label_info: Vec<LabelInfo>,
    #[serde(default)]
    media: Vec<Medium>,
}

impl Default for ReleaseItem {
    fn default() -> Self {
        Self {
            id: String::new(),
            title: String::new(),
            status: None,
            country: None,
            date: None,
            disambiguation: String::new(),
            release_group: ReleaseGroup::default(),
            artist_credit: Vec::new(),
            label_info: Vec::new(),
            media: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
struct ReleaseGroup {
    id: String,
    #[serde(rename = "primary-type")]
    primary_type: Option<String>,
    #[serde(rename = "secondary-types")]
    secondary_types: Option<Vec<String>>,
    #[serde(rename = "first-release-date")]
    first_release_date: Option<String>,
}

impl Default for ReleaseGroup {
    fn default() -> Self {
        Self {
            id: String::new(),
            primary_type: None,
            secondary_types: Some(Vec::new()),
            first_release_date: None,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
struct ArtistCredit {
    artist: ArtistSubRecord,
}

impl Default for ArtistCredit {
    fn default() -> Self {
        Self {
            artist: ArtistSubRecord::default(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
struct ArtistSubRecord {
    name: String,
}

impl Default for ArtistSubRecord {
    fn default() -> Self {
        Self {
            name: String::new(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
struct LabelInfo {
    #[serde(default)]
    label: Option<LabelRecord>,
}

impl Default for LabelInfo {
    fn default() -> Self {
        Self { label: None }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
struct LabelRecord {
    name: Option<String>,
}

impl Default for LabelRecord {
    fn default() -> Self {
        Self {
            name: Some(String::new()),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
struct Medium {
    #[serde(rename = "track-count")]
    track_count: i32,
    tracks: Vec<TrackItem>,
}

impl Default for Medium {
    fn default() -> Self {
        Self {
            track_count: 0,
            tracks: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
struct TrackItem {
    id: String,
    #[serde(default)]
    position: Option<TextOrNumber>,
    #[serde(default)]
    number: Option<TextOrNumber>,
    #[serde(default)]
    title: String,
    length: Option<i64>,
}

impl Default for TrackItem {
    fn default() -> Self {
        Self {
            id: String::new(),
            position: None,
            number: None,
            title: String::new(),
            length: Some(0),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
enum TextOrNumber {
    Text(String),
    Number(i64),
}

impl TextOrNumber {
    fn into_string(self) -> String {
        match self {
            TextOrNumber::Text(value) => value,
            TextOrNumber::Number(value) => value.to_string(),
        }
    }
}

impl From<TrackItem> for TrackInfo {
    fn from(track: TrackItem) -> Self {
        let position = track
            .position
            .map(TextOrNumber::into_string)
            .filter(|value| !value.is_empty())
            .or_else(|| track.number.map(TextOrNumber::into_string))
            .unwrap_or_default();

        Self {
            position,
            title: track.title,
            length_ms: track.length.unwrap_or_default(),
        }
    }
}
