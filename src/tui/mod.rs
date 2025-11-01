mod controller;
mod state;
mod ui;

use std::io;
use std::time::Duration;

use anyhow::Result;
use crossterm::{
    event::{Event, EventStream, KeyCode, KeyEvent, KeyModifiers},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use futures::StreamExt;
use ratatui::{Terminal, backend::CrosstermBackend};
use tokio::time::interval;

pub use controller::AppController;
pub use state::{App, FocusArea};

pub async fn run(mut app: App) -> Result<()> {
    app.bootstrap()?;

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    terminal.hide_cursor()?;

    let mut reader = EventStream::new();
    let mut ticker = interval(Duration::from_millis(200));

    loop {
        terminal.draw(|frame| ui::draw(frame, &mut app))?;

        tokio::select! {
            _ = ticker.tick() => {},
            maybe_event = reader.next() => {
                if let Some(Ok(event)) = maybe_event {
                    handle_event(&mut app, event)?;
                }
            }
            Some(message) = app.msg_rx.recv() => {
                app.handle_message(message);
            }
        }

        if app.should_quit {
            break;
        }
    }

    terminal.show_cursor()?;
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    Ok(())
}

fn handle_event(app: &mut App, event: Event) -> Result<()> {
    match event {
        Event::Key(key_event) => handle_key_event(app, key_event)?,
        Event::Resize(_, _) => {}
        _ => {}
    }
    Ok(())
}

fn handle_key_event(app: &mut App, key: KeyEvent) -> Result<()> {
    match key.code {
        KeyCode::Char('q') if app.focus != FocusArea::ManualAdd => {
            app.should_quit = true;
        }
        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            app.should_quit = true;
        }
        KeyCode::Char('m') if key.modifiers.contains(KeyModifiers::CONTROL) && app.focus != FocusArea::ManualAdd => {
            // Ctrl+M to open manual add mode
            app.focus = FocusArea::ManualAdd;
            app.manual_add_input.clear();
        }
        KeyCode::Esc => {
            if app.focus == FocusArea::ManualAdd {
                app.focus = FocusArea::Library;
                app.manual_add_input.clear();
            } else if app.focus == FocusArea::Search {
                app.search_input.clear();
            } else if app.focus == FocusArea::Albums {
                app.selected_album_ids.clear();
            }
        }
        KeyCode::Tab if app.focus != FocusArea::ManualAdd => app.next_focus(),
        KeyCode::BackTab if app.focus != FocusArea::ManualAdd => app.previous_focus(),
        _ => match app.focus {
            FocusArea::Search => handle_search_keys(app, key)?,
            FocusArea::Artists => handle_artists_keys(app, key),
            FocusArea::Albums => handle_albums_keys(app, key)?,
            FocusArea::Library => handle_library_keys(app, key)?,
            FocusArea::Logs => {}
            FocusArea::ManualAdd => handle_manual_add_keys(app, key)?,
        },
    }
    Ok(())
}

fn handle_search_keys(app: &mut App, key: KeyEvent) -> Result<()> {
    match key.code {
        KeyCode::Enter => {
            app.controller.search_artists(app.search_input.clone());
        }
        KeyCode::Backspace => {
            app.search_input.pop();
        }
        KeyCode::Char(ch) => {
            if !key.modifiers.contains(KeyModifiers::ALT)
                && !key.modifiers.contains(KeyModifiers::CONTROL)
            {
                app.search_input.push(ch);
            }
        }
        _ => {}
    }
    Ok(())
}

fn handle_artists_keys(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Up => app.move_artist_selection(-1),
        KeyCode::Down => app.move_artist_selection(1),
        KeyCode::Enter => {
            if let Some(artist) = app.selected_artist() {
                app.controller.load_albums_for_artist(artist);
            }
        }
        _ => {}
    }
}

fn handle_albums_keys(app: &mut App, key: KeyEvent) -> Result<()> {
    match key.code {
        KeyCode::Up => app.move_album_selection(-1),
        KeyCode::Down => app.move_album_selection(1),
        KeyCode::Char(' ') => {
            app.toggle_album_selection();
        }
        KeyCode::Char('a') => {
            let albums = app.selected_albums();
            if albums.is_empty() {
                app.push_log("No albums selected");
            } else {
                app.controller.add_albums(albums)?;
            }
        }
        _ => {}
    }
    Ok(())
}

fn handle_library_keys(app: &mut App, key: KeyEvent) -> Result<()> {
    match key.code {
        KeyCode::Up => app.move_library_selection(-1),
        KeyCode::Down => app.move_library_selection(1),
        KeyCode::Char('g') => {
            let pending: Vec<_> = app
                .library
                .iter()
                .filter(|record| {
                    // Only include albums with metadata (artist name populated)
                    !record.artist.is_empty() && record.note_path.is_none()
                })
                .cloned()
                .collect();
            if pending.is_empty() {
                let has_albums = !app.library.is_empty();
                if has_albums {
                    app.push_log("All notes already generated (or metadata still loading)");
                } else {
                    app.push_log("No albums in library");
                }
            } else {
                app.controller.generate_notes(pending);
            }
        }
        _ => {}
    }
    Ok(())
}

fn handle_manual_add_keys(app: &mut App, key: KeyEvent) -> Result<()> {
    match key.code {
        KeyCode::Enter => {
            let release_id = app.manual_add_input.trim().to_string();
            if !release_id.is_empty() {
                app.controller.add_album_by_release_id(release_id);
                app.manual_add_input.clear();
                app.focus = FocusArea::Library;
            }
        }
        KeyCode::Backspace => {
            app.manual_add_input.pop();
        }
        KeyCode::Char(ch) => {
            if !key.modifiers.contains(KeyModifiers::ALT)
                && !key.modifiers.contains(KeyModifiers::CONTROL)
            {
                app.manual_add_input.push(ch);
            }
        }
        _ => {}
    }
    Ok(())
}
