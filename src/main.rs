mod app;
mod book;
mod config;
mod catalog;
mod layout;
mod state;
mod theme;
mod ui;

use app::{App, ScreenMode};
use crossterm::event::{self, Event};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use std::io;
use std::path::PathBuf;
use std::time::Duration;

fn restore_terminal() {
    let _ = disable_raw_mode();
    let _ = execute!(io::stdout(), LeaveAlternateScreen);
}

fn main() -> io::Result<()> {
    let initial_file = std::env::args_os().nth(1).map(PathBuf::from);

    let orig_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        restore_terminal();
        orig_hook(info);
    }));

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new(initial_file);
    let mut needs_redraw = true;

    while !app.should_quit {
        let size = terminal.size()?;
        app.ensure_layout(size.width, size.height);

        if needs_redraw {
            terminal.draw(|frame| ui::draw(frame, &mut app))?;
            needs_redraw = false;
        }

        let poll_timeout = if app.mode == ScreenMode::Rsvp && app.rsvp_playing {
            Duration::from_millis(16)
        } else if app.esc_hold_start.is_some() || app.gut_busy {
            Duration::from_millis(100)
        } else {
            Duration::from_millis(250)
        };

        if event::poll(poll_timeout)? {
            match event::read()? {
                Event::Key(key) => {
                    app.handle_key(key);
                    needs_redraw = true;
                }
                Event::Resize(w, h) => {
                    app.ensure_layout(w, h);
                    needs_redraw = true;
                }
                _ => {}
            }
        }

        if app.tick() {
            needs_redraw = true;
        }
    }

    restore_terminal();
    Ok(())
}
