use std::collections::{HashSet, VecDeque};

use anyhow::Result;
use ratatui::widgets::ListState;
use tokio::sync::mpsc::UnboundedReceiver;

use crate::app::AppMessage;
use crate::models::{Album, AlbumRecord, Artist};

use super::controller::AppController;

const LOG_CAPACITY: usize = 200;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FocusArea {
    Search,
    Artists,
    Albums,
    Library,
    Logs,
    ManualAdd,
}

impl FocusArea {
    pub fn next(self) -> Self {
        match self {
            FocusArea::Search => FocusArea::Artists,
            FocusArea::Artists => FocusArea::Albums,
            FocusArea::Albums => FocusArea::Library,
            FocusArea::Library => FocusArea::Logs,
            FocusArea::Logs => FocusArea::Search,
            FocusArea::ManualAdd => FocusArea::ManualAdd, // Stay in manual add mode
        }
    }

    pub fn previous(self) -> Self {
        match self {
            FocusArea::Search => FocusArea::Logs,
            FocusArea::Artists => FocusArea::Search,
            FocusArea::Albums => FocusArea::Artists,
            FocusArea::Library => FocusArea::Albums,
            FocusArea::Logs => FocusArea::Library,
            FocusArea::ManualAdd => FocusArea::ManualAdd, // Stay in manual add mode
        }
    }
}

pub struct App {
    pub controller: AppController,
    pub msg_rx: UnboundedReceiver<AppMessage>,
    pub search_input: String,
    pub manual_add_input: String,
    pub artist_results: Vec<Artist>,
    pub artist_state: ListState,
    pub albums: Vec<Album>,
    pub album_state: ListState,
    pub selected_album_ids: HashSet<String>,
    pub library: Vec<AlbumRecord>,
    pub library_state: ListState,
    pub logs: VecDeque<String>,
    pub focus: FocusArea,
    pub should_quit: bool,
}

impl App {
    pub fn new(controller: AppController, msg_rx: UnboundedReceiver<AppMessage>) -> Self {
        let mut artist_state = ListState::default();
        artist_state.select(None);
        let mut album_state = ListState::default();
        album_state.select(None);
        let mut library_state = ListState::default();
        library_state.select(None);

        Self {
            controller,
            msg_rx,
            search_input: String::new(),
            manual_add_input: String::new(),
            artist_results: Vec::new(),
            artist_state,
            albums: Vec::new(),
            album_state,
            selected_album_ids: HashSet::new(),
            library: Vec::new(),
            library_state,
            logs: VecDeque::with_capacity(LOG_CAPACITY),
            focus: FocusArea::Search,
            should_quit: false,
        }
    }

    pub fn bootstrap(&mut self) -> Result<()> {
        self.library = self.controller.load_library()?;
        if !self.library.is_empty() {
            self.library_state.select(Some(0));
        }
        Ok(())
    }

    pub fn handle_message(&mut self, message: AppMessage) {
        match message {
            AppMessage::ArtistResults(results) => {
                self.artist_results = results;
                self.artist_state.select(if self.artist_results.is_empty() {
                    None
                } else {
                    Some(0)
                });
                self.focus = FocusArea::Artists;
                self.push_log("Artist search completed");
            }
            AppMessage::AlbumsLoaded(albums) => {
                self.albums = albums;
                self.album_state.select(if self.albums.is_empty() {
                    None
                } else {
                    Some(0)
                });
                self.selected_album_ids.clear();
                self.focus = FocusArea::Albums;
                self.push_log("Albums loaded");
            }
            AppMessage::SearchFailed(reason) => {
                self.push_log(reason);
            }
            AppMessage::CoverArtStatus { mbid, status, path } => {
                if let Some(record) = self.library.iter_mut().find(|record| record.mbid == mbid) {
                    record.cover_art_status = status;
                    record.cover_art_path = path.clone();
                }
            }
            AppMessage::DownloadLog(entry) => {
                self.push_log(entry);
            }
            AppMessage::LibraryRefreshed(records) => {
                self.library = records;
                if !self.library.is_empty() {
                    let idx = self
                        .library_state
                        .selected()
                        .unwrap_or(0)
                        .min(self.library.len() - 1);
                    self.library_state.select(Some(idx));
                } else {
                    self.library_state.select(None);
                }
            }
            AppMessage::NotesGenerated(logs) => {
                for log in logs {
                    self.push_log(log);
                }
            }
        }
    }

    pub fn next_focus(&mut self) {
        self.focus = self.focus.next();
    }

    pub fn previous_focus(&mut self) {
        self.focus = self.focus.previous();
    }

    pub fn push_log<S: Into<String>>(&mut self, message: S) {
        if self.logs.len() == LOG_CAPACITY {
            self.logs.pop_front();
        }
        self.logs.push_back(message.into());
    }

    pub fn selected_artist(&self) -> Option<Artist> {
        self.artist_state
            .selected()
            .and_then(|idx| self.artist_results.get(idx).cloned())
    }

    pub fn selected_album(&self) -> Option<Album> {
        self.album_state
            .selected()
            .and_then(|idx| self.albums.get(idx).cloned())
    }

    pub fn toggle_album_selection(&mut self) {
        if let Some(album) = self.selected_album() {
            if self.selected_album_ids.contains(&album.id) {
                self.selected_album_ids.remove(&album.id);
            } else {
                self.selected_album_ids.insert(album.id.clone());
            }
        }
    }

    pub fn selected_albums(&self) -> Vec<Album> {
        self.albums
            .iter()
            .filter(|album| self.selected_album_ids.contains(&album.id))
            .cloned()
            .collect()
    }

    pub fn move_artist_selection(&mut self, delta: isize) {
        let len = self.artist_results.len();
        update_list_state(&mut self.artist_state, len, delta);
    }

    pub fn move_album_selection(&mut self, delta: isize) {
        let len = self.albums.len();
        update_list_state(&mut self.album_state, len, delta);
    }

    pub fn move_library_selection(&mut self, delta: isize) {
        let len = self.library.len();
        update_list_state(&mut self.library_state, len, delta);
    }
}

fn update_list_state(state: &mut ListState, len: usize, delta: isize) {
    if len == 0 {
        state.select(None);
        return;
    }

    let current = state.selected().unwrap_or(0);
    let step = delta.abs() as usize;
    let new_index = if delta < 0 {
        current.saturating_sub(step)
    } else {
        (current + step).min(len - 1)
    };
    state.select(Some(new_index));
}
