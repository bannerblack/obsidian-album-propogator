use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::Line,
    widgets::{Block, Borders, List, ListItem, Paragraph, Wrap},
};

use crate::models::{Album, AlbumRecord, Artist, CoverArtStatus};

use super::{App, state::FocusArea};

pub fn draw(frame: &mut Frame, app: &mut App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(10),
            Constraint::Length(1),
        ])
        .split(frame.size());

    draw_search(frame, app, chunks[0]);

    let body_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(60), Constraint::Percentage(40)])
        .split(chunks[1]);

    let upper = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
        .split(body_chunks[0]);

    draw_artist_list(frame, app, upper[0]);
    draw_album_list(frame, app, upper[1]);

    let lower = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(55), Constraint::Percentage(45)])
        .split(body_chunks[1]);

    draw_library(frame, app, lower[0]);
    draw_logs(frame, app, lower[1]);

    draw_footer(frame, chunks[2]);

    // Draw manual add dialog on top if active
    if app.focus == FocusArea::ManualAdd {
        draw_manual_add_dialog(frame, app);
    }
}

fn draw_search(frame: &mut Frame, app: &App, area: Rect) {
    let block = Block::default()
        .title("Search Artist")
        .borders(Borders::ALL)
        .border_style(border_style(app.focus, FocusArea::Search));

    let paragraph = Paragraph::new(format!("> {}", app.search_input))
        .block(block)
        .wrap(Wrap { trim: false });

    frame.render_widget(paragraph, area);
}

fn draw_artist_list(frame: &mut Frame, app: &mut App, area: Rect) {
    let items: Vec<ListItem> = if app.artist_results.is_empty() {
        vec![ListItem::new("No artists loaded").style(dim_style())]
    } else {
        app.artist_results
            .iter()
            .map(|artist| ListItem::new(artist_line(artist)))
            .collect()
    };

    let list = List::new(items)
        .block(
            Block::default()
                .title("Artists")
                .borders(Borders::ALL)
                .border_style(border_style(app.focus, FocusArea::Artists)),
        )
        .highlight_style(
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("▶ ");

    frame.render_stateful_widget(list, area, &mut app.artist_state);
}

fn draw_album_list(frame: &mut Frame, app: &mut App, area: Rect) {
    let items: Vec<ListItem> = if app.albums.is_empty() {
        vec![ListItem::new("No albums loaded").style(dim_style())]
    } else {
        app.albums
            .iter()
            .map(|album| {
                ListItem::new(album_lines(
                    album,
                    app.selected_album_ids.contains(&album.id),
                ))
            })
            .collect()
    };

    let list = List::new(items)
        .block(
            Block::default()
                .title("Albums")
                .borders(Borders::ALL)
                .border_style(border_style(app.focus, FocusArea::Albums)),
        )
        .highlight_style(
            Style::default()
                .fg(Color::LightGreen)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("▶ ");

    frame.render_stateful_widget(list, area, &mut app.album_state);
}

fn draw_library(frame: &mut Frame, app: &mut App, area: Rect) {
    let items: Vec<ListItem> = if app.library.is_empty() {
        vec![ListItem::new("Library is empty").style(dim_style())]
    } else {
        app.library
            .iter()
            .map(|record| ListItem::new(library_lines(record)))
            .collect()
    };

    let list = List::new(items)
        .block(
            Block::default()
                .title("Library")
                .borders(Borders::ALL)
                .border_style(border_style(app.focus, FocusArea::Library)),
        )
        .highlight_style(
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("▶ ");

    frame.render_stateful_widget(list, area, &mut app.library_state);
}

fn draw_logs(frame: &mut Frame, app: &App, area: Rect) {
    let lines: Vec<Line> = app
        .logs
        .iter()
        .rev()
        .take(100)
        .map(|entry| Line::from(entry.clone()))
        .collect();

    let paragraph = Paragraph::new(lines)
        .block(
            Block::default()
                .title("Activity")
                .borders(Borders::ALL)
                .border_style(border_style(app.focus, FocusArea::Logs)),
        )
        .wrap(Wrap { trim: true });

    frame.render_widget(paragraph, area);
}

fn draw_footer(frame: &mut Frame, area: Rect) {
    let footer = Paragraph::new(
        "Tab: cycle • Enter: confirm • Space: toggle • a: add albums • g: generate notes • Ctrl+M: manual add • q: quit",
    )
    .style(Style::default().fg(Color::Gray));
    frame.render_widget(footer, area);
}

fn draw_manual_add_dialog(frame: &mut Frame, app: &App) {
    use ratatui::layout::Alignment;

    // Center the dialog
    let area = frame.size();
    let dialog_width = 60.min(area.width - 4);
    let dialog_height = 5;
    let x = (area.width.saturating_sub(dialog_width)) / 2;
    let y = (area.height.saturating_sub(dialog_height)) / 2;

    let dialog_area = Rect {
        x,
        y,
        width: dialog_width,
        height: dialog_height,
    };

    // Clear the area
    let clear_block = Block::default()
        .style(Style::default().bg(Color::Black));
    frame.render_widget(clear_block, dialog_area);

    // Draw the dialog
    let block = Block::default()
        .title("Add Album by Release ID")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD));

    let inner = block.inner(dialog_area);
    frame.render_widget(block, dialog_area);

    let text = vec![
        Line::from("Enter MusicBrainz Release ID:"),
        Line::from(format!("> {}", app.manual_add_input)),
        Line::from(""),
        Line::from("Press Enter to add, Esc to cancel")
            .style(Style::default().fg(Color::DarkGray)),
    ];

    let paragraph = Paragraph::new(text)
        .alignment(Alignment::Left);

    frame.render_widget(paragraph, inner);
}

fn artist_line(artist: &Artist) -> Line<'static> {
    let text = artist.display_name();
    Line::from(text)
}

fn album_lines(album: &Album, selected: bool) -> Vec<Line<'static>> {
    let marker = if selected { "[x]" } else { "[ ]" };
    vec![
        Line::from(format!(
            "{marker} {} ({})",
            album.title, album.first_release_date
        )),
        Line::from(format!(
            "   {} • {}",
            album.primary_type,
            album.secondary_types_label()
        )),
    ]
}

fn library_lines(record: &AlbumRecord) -> Vec<Line<'static>> {
    let status = match record.cover_art_status {
        CoverArtStatus::Completed => "Art: ✔",
        CoverArtStatus::Queued | CoverArtStatus::Pending => "Art: ⏳",
        CoverArtStatus::Downloading => "Art: ↓",
        CoverArtStatus::Unavailable => "Art: ✖",
    };

    let notes = if record.note_path.is_some() {
        "Notes: ✔"
    } else {
        "Notes: ⏳"
    };

    vec![
        Line::from(format!("{} — {}", record.artist, record.title)),
        Line::from(format!("   {status} • {notes}")),
    ]
}

fn border_style(current: FocusArea, area: FocusArea) -> Style {
    if current == area {
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default()
    }
}

fn dim_style() -> Style {
    Style::default().fg(Color::DarkGray)
}
