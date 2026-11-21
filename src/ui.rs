use crate::app::{App, ScreenMode};
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, List, ListItem, ListState, Paragraph};
use ratatui::Frame;

pub fn draw(frame: &mut Frame, app: &mut App) {
    let area = frame.area();
    let theme = app.theme();

    frame.render_widget(Clear, area);
    frame.render_widget(
        Block::default().style(Style::default().bg(theme.bg).fg(theme.fg)),
        area,
    );

    match app.mode {
        ScreenMode::Library => draw_library(frame, app, area),
        ScreenMode::Reader => draw_reader(frame, app, area),
        ScreenMode::Toc => {
            draw_reader(frame, app, area);
            draw_toc(frame, app, area);
        }
        ScreenMode::Bookmarks => {
            draw_reader(frame, app, area);
            draw_bookmarks(frame, app, area);
        }
        ScreenMode::Note => {
            draw_reader(frame, app, area);
            draw_note(frame, app, area);
        }
        ScreenMode::Search => {
            draw_reader(frame, app, area);
            draw_search(frame, app, area);
        }
        ScreenMode::Gutenberg => draw_gutenberg(frame, app, area),
        ScreenMode::Help => {
            draw_reader(frame, app, area);
            draw_help(frame, app, area);
        }
        ScreenMode::Rsvp => draw_rsvp(frame, app, area),
    }
}

fn status_bar_text(app: &App, default_text: String) -> String {
    if let Some(rem) = app.esc_hold_remaining_secs() {
        if rem > 0 && app.esc_hold_start.is_some_and(|s| s.elapsed().as_millis() > 350) {
            return format!(" Hold ESC to quit ({rem}s)...");
        }
    }
    default_text
}

fn draw_library(frame: &mut Frame, app: &App, area: Rect) {
    let theme = app.theme();
    let [header_area, body_area, footer_area] = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(1),
            Constraint::Length(1),
        ])
        .areas(area);

    let header = Paragraph::new(Line::from(vec![
        Span::styled(
            " zread ",
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("· {} books · {}p read", app.library.len(), app.state.pages_read),
            Style::default().fg(theme.muted),
        ),
    ]));
    frame.render_widget(header, header_area);

    if app.library.is_empty() {
        let empty = Paragraph::new("No .epub/.md/.txt files in library")
            .style(Style::default().fg(theme.muted))
            .alignment(Alignment::Center);
        frame.render_widget(empty, body_area);
    } else {
        let items: Vec<ListItem> = app
            .library
            .iter()
            .map(|entry| {
                let mark = if entry.protected {
                    "!"
                } else if entry.has_progress {
                    "*"
                } else {
                    " "
                };
                ListItem::new(Line::from(vec![
                    Span::styled(format!("{mark}[{}] ", entry.ext), Style::default().fg(theme.muted)),
                    Span::styled(&entry.name, Style::default().fg(theme.fg)),
                ]))
            })
            .collect();

        let list = List::new(items)
            .highlight_style(
                Style::default()
                    .bg(theme.status_bg)
                    .fg(theme.accent)
                    .add_modifier(Modifier::BOLD),
            )
            .highlight_symbol(">");

        let mut list_state = ListState::default();
        list_state.select(Some(app.lib_selected));
        frame.render_stateful_widget(list, body_area, &mut list_state);
    }

    let default_status = app.status_msg.clone().unwrap_or_else(|| {
        if app.book.is_some() {
            " f/x:nav c:open g:get d:del v:theme Esc:back".to_string()
        } else {
            " f/x:nav c:open g:get d:del v:theme Esc3s:quit".to_string()
        }
    });
    let footer = Paragraph::new(status_bar_text(app, default_status))
        .style(Style::default().bg(theme.status_bg).fg(theme.status_fg));
    frame.render_widget(footer, footer_area);

    if let Some(target) = &app.lib_confirm_delete {
        let popup = centered_rect(90, 45, area);
        frame.render_widget(Clear, popup);
        let msg = vec![
            Line::from(Span::styled(
                format!("Delete {}?", truncate_with_ellipsis(&target.name, 26)),
                Style::default().fg(theme.accent).add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
            Line::from("y / Enter : Yes   n / Esc : No"),
        ];
        let confirm = Paragraph::new(msg)
            .alignment(Alignment::Center)
            .block(
                Block::default()
                    .title(" Confirm ")
                    .borders(ratatui::widgets::Borders::ALL)
                    .style(Style::default().bg(theme.bg).fg(theme.fg)),
            );
        frame.render_widget(confirm, popup);
    }
}

fn draw_reader(frame: &mut Frame, app: &App, area: Rect) {
    let theme = app.theme();
    let [header_area, body_area, footer_area] = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(1),
            Constraint::Length(1),
        ])
        .areas(area);

    let margin = app.margin_chars();
    let text_area = Rect {
        x: body_area.x.saturating_add(margin),
        y: body_area.y,
        width: body_area.width.saturating_sub(margin.saturating_mul(2)),
        height: body_area.height,
    };

    if let (Some(book), Some(pag)) = (&app.book, &app.paginated) {
        if let Some(page) = pag.pages.get(app.current_page) {
            let total_pages = pag.pages.len().max(1);
            let pct = ((app.current_page + 1) * 100) / total_pages;

            // Pala One-style inverted top bar: truncated title left, percentage right
            let right_label = format!(" {pct}% ");
            let max_title_len = (header_area.width as usize).saturating_sub(right_label.len() + 2);
            let title_str = truncate_with_ellipsis(&book.title, max_title_len);
            let pad = (header_area.width as usize)
                .saturating_sub(title_str.chars().count() + right_label.len() + 1);
            let top_line = format!(" {title_str}{}{right_label}", " ".repeat(pad));
            frame.render_widget(
                Paragraph::new(top_line)
                    .style(Style::default().bg(theme.status_bg).fg(theme.accent)),
                header_area,
            );

            let lines: Vec<Line> = page
                .lines
                .iter()
                .map(|l| Line::from(Span::styled(l.as_str(), Style::default().fg(theme.fg))))
                .collect();
            frame.render_widget(Paragraph::new(lines), text_area);

            let ch_num = page.start_chapter + 1;
            let ch_total = book.chapters.len().max(1);
            let info = format!(
                "Ch{}/{} {}/{} t:toc b:mk ?:help",
                ch_num,
                ch_total,
                app.current_page + 1,
                total_pages
            );
            let bar_w = (footer_area.width as usize).saturating_sub(info.chars().count() + 3);
            let default_status = if bar_w >= 6 {
                let filled = ((app.current_page + 1) * bar_w) / total_pages;
                let bar = format!(
                    "{}{}",
                    "━".repeat(filled),
                    "─".repeat(bar_w.saturating_sub(filled))
                );
                format!(" {bar} {info}")
            } else {
                format!(" {info}")
            };
            let footer = Paragraph::new(status_bar_text(app, default_status))
                .style(Style::default().bg(theme.status_bg).fg(theme.status_fg));
            frame.render_widget(footer, footer_area);
        }
    }
}

fn truncate_with_ellipsis(s: &str, max_chars: usize) -> String {
    let count = s.chars().count();
    if count <= max_chars {
        return s.to_string();
    }
    if max_chars <= 3 {
        return s.chars().take(max_chars).collect();
    }
    let prefix: String = s.chars().take(max_chars - 3).collect();
    format!("{prefix}...")
}

fn draw_note(frame: &mut Frame, app: &App, area: Rect) {
    let theme = app.theme();
    let popup = centered_rect(92, 55, area);
    frame.render_widget(Clear, popup);

    let widget = Paragraph::new(format!("{}_", app.note_buf))
        .wrap(ratatui::widgets::Wrap { trim: false })
        .block(
            Block::default()
                .title(" Quick Note (~/Books/Notes) ")
                .title_bottom(" Enter:save  Esc:cancel ")
                .borders(ratatui::widgets::Borders::ALL)
                .style(Style::default().bg(theme.bg).fg(theme.accent)),
        )
        .style(Style::default().fg(theme.fg));
    frame.render_widget(widget, popup);
}

fn draw_bookmarks(frame: &mut Frame, app: &App, area: Rect) {
    let theme = app.theme();
    let popup = centered_rect(94, 80, area);
    frame.render_widget(Clear, popup);

    let Some(book) = &app.book else {
        return;
    };
    let bookmarks = app.state.get_bookmarks(&book.path);
    let block = Block::default()
        .title(" Bookmarks ")
        .title_bottom(" f/x:nav c:go m:add d:del Esc:back ")
        .borders(ratatui::widgets::Borders::ALL)
        .style(Style::default().bg(theme.bg).fg(theme.accent));

    if bookmarks.is_empty() {
        let empty = Paragraph::new("No bookmarks yet.\nPress 'm' to add one.")
            .block(block)
            .style(Style::default().fg(theme.muted))
            .alignment(Alignment::Center);
        frame.render_widget(empty, popup);
        return;
    }

    let items: Vec<ListItem> = bookmarks
        .iter()
        .map(|bm| ListItem::new(Span::styled(&bm.label, Style::default().fg(theme.fg))))
        .collect();

    let list = List::new(items)
        .block(block)
        .highlight_style(
            Style::default()
                .bg(theme.status_bg)
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD),
        );

    let mut state = ListState::default();
    state.select(Some(app.bm_selected));
    frame.render_stateful_widget(list, popup, &mut state);
}

fn draw_toc(frame: &mut Frame, app: &App, area: Rect) {
    let theme = app.theme();
    let popup = centered_rect(90, 80, area);
    frame.render_widget(Clear, popup);

    let Some(book) = &app.book else {
        return;
    };
    let items: Vec<ListItem> = book
        .chapters
        .iter()
        .enumerate()
        .map(|(i, ch)| {
            let pg = app
                .paginated
                .as_ref()
                .and_then(|p| p.chapter_start_pages.get(i))
                .map_or(1, |idx| idx + 1);
            let max_w = (popup.width as usize).saturating_sub(12);
            let label = truncate_with_ellipsis(&format!("{}. {}", i + 1, ch.title), max_w);
            ListItem::new(Line::from(vec![
                Span::styled(label, Style::default().fg(theme.fg)),
                Span::styled(format!("  p.{pg}"), Style::default().fg(theme.muted)),
            ]))
        })
        .collect();

    let list = List::new(items)
        .block(
            Block::default()
                .title(" Table of Contents ")
                .title_bottom(" f/x:nav c:jump t/Esc:back ")
                .borders(ratatui::widgets::Borders::ALL)
                .style(Style::default().bg(theme.bg).fg(theme.accent)),
        )
        .highlight_style(
            Style::default()
                .bg(theme.status_bg)
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD),
        );

    let mut state = ListState::default();
    state.select(Some(app.toc_selected));
    frame.render_stateful_widget(list, popup, &mut state);
}

fn draw_gutenberg(frame: &mut Frame, app: &App, area: Rect) {
    let theme = app.theme();
    let [header_area, body_area, footer_area] = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(1),
            Constraint::Length(1),
        ])
        .areas(area);

    let cursor = if app.gut_typing { "_" } else { "" };
    let header = Paragraph::new(Line::from(vec![
        Span::styled(
            " Online: ",
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("{}{}", app.gut_query, cursor),
            Style::default().fg(theme.fg),
        ),
    ]));
    frame.render_widget(header, header_area);

    if app.gut_results.is_empty() {
        let msg = if app.gut_busy {
            app.gut_status.as_deref().unwrap_or("Searching...")
        } else if app.gut_typing {
            "Search free ebooks (e.g. 'walden', 'sherlock')"
        } else {
            "No results"
        };
        frame.render_widget(
            Paragraph::new(msg)
                .style(Style::default().fg(if app.gut_busy { theme.accent } else { theme.muted }))
                .alignment(Alignment::Center),
            body_area,
        );
    } else {
        let items: Vec<ListItem> = app
            .gut_results
            .iter()
            .map(|b| {
                ListItem::new(Line::from(vec![
                    Span::styled(format!("[{}] ", b.source), Style::default().fg(theme.muted)),
                    Span::styled(&b.title, Style::default().fg(theme.fg)),
                    Span::styled(format!(" - {}", b.author), Style::default().fg(theme.muted)),
                ]))
            })
            .collect();

        let list = List::new(items)
            .highlight_style(
                Style::default()
                    .bg(theme.status_bg)
                    .fg(theme.accent)
                    .add_modifier(Modifier::BOLD),
            )
            .highlight_symbol(">");

        let mut state = ListState::default();
        state.select(Some(app.gut_selected));
        frame.render_stateful_widget(list, body_area, &mut state);
    }

    let status = app.gut_status.clone().unwrap_or_else(|| {
        if app.gut_typing {
            " Type query · Enter:search · Esc:library".to_string()
        } else {
            " f/x:nav c:get /:new Esc:search".to_string()
        }
    });
    frame.render_widget(
        Paragraph::new(status_bar_text(app, status))
            .style(Style::default().bg(theme.status_bg).fg(theme.status_fg)),
        footer_area,
    );
}

fn draw_search(frame: &mut Frame, app: &App, area: Rect) {
    let theme = app.theme();
    let popup = centered_rect(92, 75, area);
    frame.render_widget(Clear, popup);

    let block = Block::default()
        .title(format!(" Search: {}_ ", app.search_query))
        .title_bottom(" ↑/↓:nav Enter:jump Esc:back ")
        .borders(ratatui::widgets::Borders::ALL)
        .style(Style::default().bg(theme.bg).fg(theme.accent));
    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    let items: Vec<ListItem> = app
        .search_matches
        .iter()
        .map(|(pg, snippet)| {
            ListItem::new(Span::styled(
                format!("p{}: {}", pg + 1, snippet),
                Style::default().fg(theme.fg),
            ))
        })
        .collect();

    let list = List::new(items).highlight_style(
        Style::default()
            .bg(theme.status_bg)
            .fg(theme.accent)
            .add_modifier(Modifier::BOLD),
    );
    let mut state = ListState::default();
    if !app.search_matches.is_empty() {
        state.select(Some(app.search_selected));
    }
    frame.render_stateful_widget(list, inner, &mut state);
}

fn draw_help(frame: &mut Frame, app: &App, area: Rect) {
    let theme = app.theme();
    let popup = centered_rect(94, 88, area);
    frame.render_widget(Clear, popup);

    let help_lines = vec![
        Line::from(Span::styled("Reader", Style::default().fg(theme.accent).add_modifier(Modifier::BOLD))),
        Line::from(" c/x/Space/→/↓ : Next page"),
        Line::from(" z/f/←/↑       : Prev page"),
        Line::from(" t             : Table of Contents"),
        Line::from(" [ / ]         : Prev / Next chapter"),
        Line::from(" g / G         : First / Last page"),
        Line::from(" m / b         : Add / Manage bookmarks"),
        Line::from(" N             : Add quick note"),
        Line::from(" /             : Search in book"),
        Line::from(" r             : RSVP speed-read"),
        Line::from(" v             : Cycle color theme"),
        Line::from(" M             : Cycle margins (1/2/4)"),
        Line::from(" P             : Toggle compact paras"),
        Line::from(" l             : Open Library"),
        Line::from(Span::styled("Library & Online", Style::default().fg(theme.accent).add_modifier(Modifier::BOLD))),
        Line::from(" f/x           : Move selection"),
        Line::from(" c/Enter       : Open / Download book"),
        Line::from(" g             : Search online (SE/PG/IA)"),
        Line::from(" d             : Delete book (non-help)"),
        Line::from(" R             : Refresh library"),
        Line::from(Span::styled("RSVP & General", Style::default().fg(theme.accent).add_modifier(Modifier::BOLD))),
        Line::from(" Space         : Pause / Resume RSVP"),
        Line::from(" f/x or +/-    : RSVP WPM (+/- 20)"),
        Line::from(" Esc           : Back out of menu"),
        Line::from(" Hold Esc (3s) : Save & Quit"),
    ];

    let max_scroll = help_lines
        .len()
        .saturating_sub(popup.height.saturating_sub(2) as usize);
    let scroll = app.help_scroll.min(max_scroll) as u16;

    let widget = Paragraph::new(help_lines)
        .scroll((scroll, 0))
        .block(
            Block::default()
                .title(" Commands ")
                .title_bottom(" f/x:scroll ?/Esc:close ")
                .borders(ratatui::widgets::Borders::ALL)
                .style(Style::default().bg(theme.bg).fg(theme.fg)),
        );
    frame.render_widget(widget, popup);
}

fn draw_rsvp(frame: &mut Frame, app: &App, area: Rect) {
    let theme = app.theme();
    let [_, word_area, _, footer_area] = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(1),
            Constraint::Length(1),
            Constraint::Min(1),
            Constraint::Length(1),
        ])
        .areas(area);

    let word = app
        .rsvp_words
        .get(app.rsvp_idx)
        .map_or("", |(w, _, _)| w.as_str());

    let chars: Vec<char> = word.chars().collect();
    let orp = match chars.len() {
        0..=1 => 0,
        2..=5 => 1,
        6..=9 => 2,
        _ => 3,
    };

    let prefix: String = chars.get(..orp.min(chars.len())).unwrap_or_default().iter().collect();
    let focus: String = chars.get(orp).map_or_default(std::string::ToString::to_string);
    let suffix: String = chars
        .get(orp.saturating_add(1)..)
        .unwrap_or_default()
        .iter()
        .collect();

    let line = Line::from(vec![
        Span::styled(prefix, Style::default().fg(theme.fg)),
        Span::styled(
            focus,
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(suffix, Style::default().fg(theme.fg)),
    ]);

    frame.render_widget(Paragraph::new(line).alignment(Alignment::Center), word_area);

    let state_label = if app.rsvp_playing { "PLAY" } else { "PAUSE" };
    let default_status = format!(
        " RSVP[{state_label}] {}WPM f/x:wpm z/c:±10 Spc:play r/Esc:exit",
        app.state.wpm
    );
    frame.render_widget(
        Paragraph::new(status_bar_text(app, default_status))
            .style(Style::default().bg(theme.status_bg).fg(theme.status_fg)),
        footer_area,
    );
}

fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let [_, v_mid, _] = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .areas(area);

    let [_, center, _] = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .areas(v_mid);
    center
}
