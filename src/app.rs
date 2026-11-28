use crate::book::Book;
use crate::config::Config;
use crate::gutenberg::{download_book, search_gutenberg, GutenbergBook};
use crate::layout::{find_page_for_position, paginate_book, PaginatedBook};
use crate::state::AppState;
use crate::theme::{theme_index_by_name, Theme, THEMES};
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScreenMode {
    Library,
    Reader,
    Toc,
    Bookmarks,
    Note,
    Search,
    Gutenberg,
    Help,
    Rsvp,
}

#[derive(Debug, Clone)]
pub struct LibraryEntry {
    pub name: String,
    pub ext: String,
    pub path: PathBuf,
    pub has_progress: bool,
    pub protected: bool,
}

pub struct App {
    pub mode: ScreenMode,
    pub should_quit: bool,
    pub config: Config,
    pub state: AppState,
    pub library: Vec<LibraryEntry>,
    pub lib_selected: usize,
    pub lib_confirm_delete: Option<LibraryEntry>,
    pub status_msg: Option<String>,

    // Active book state
    pub book: Option<Book>,
    pub paginated: Option<PaginatedBook>,
    pub current_page: usize,
    pub last_size: (u16, u16),

    // TOC state
    pub toc_selected: usize,

    // Help scroll state
    pub help_scroll: usize,

    // Bookmarks state
    pub bm_selected: usize,

    // Quick Note state
    pub note_buf: String,

    // Search state
    pub search_query: String,
    pub search_matches: Vec<(usize, String)>,
    pub search_selected: usize,

    // Gutenberg search state
    pub gut_query: String,
    pub gut_typing: bool,
    pub gut_results: Vec<GutenbergBook>,
    pub gut_selected: usize,
    pub gut_status: Option<String>,
    pub gut_busy: bool,
    pub gut_search_rx: Option<Receiver<Result<Vec<GutenbergBook>, String>>>,
    pub gut_dl_rx: Option<Receiver<Result<PathBuf, String>>>,

    // RSVP speed-reader state
    pub rsvp_words: Vec<(String, usize, usize)>,
    pub rsvp_idx: usize,
    pub rsvp_playing: bool,
    pub rsvp_last_tick: Instant,

    // ESC 3-second hold-to-quit state
    pub esc_hold_start: Option<Instant>,
    pub esc_last_seen: Option<Instant>,
}

impl App {
    pub fn new(initial_file: Option<PathBuf>) -> Self {
        let config = Config::load_or_create();
        let mut state = AppState::load();
        if state.last_book.is_none() {
            state.theme_idx = theme_index_by_name(&config.theme);
            state.wpm = config.wpm;
        }

        let mut app = Self {
            mode: ScreenMode::Library,
            should_quit: false,
            config,
            state,
            library: Vec::new(),
            lib_selected: 0,
            lib_confirm_delete: None,
            status_msg: None,
            book: None,
            paginated: None,
            current_page: 0,
            last_size: (0, 0),
            toc_selected: 0,
            help_scroll: 0,
            bm_selected: 0,
            note_buf: String::new(),
            search_query: String::new(),
            search_matches: Vec::new(),
            search_selected: 0,
            gut_query: String::new(),
            gut_typing: true,
            gut_results: Vec::new(),
            gut_selected: 0,
            gut_status: None,
            gut_busy: false,
            gut_search_rx: None,
            gut_dl_rx: None,
            rsvp_words: Vec::new(),
            rsvp_idx: 0,
            rsvp_playing: false,
            rsvp_last_tick: Instant::now(),
            esc_hold_start: None,
            esc_last_seen: None,
        };

        app.refresh_library();

        if let Some(path) = initial_file {
            app.open_book(&path);
        } else if app.config.resume_last_book {
            if let Some(last) = app.state.last_book.clone() {
                let p = PathBuf::from(last);
                if p.exists() {
                    if let Some(pos) = app.library.iter().position(|e| e.path == p) {
                        app.lib_selected = pos;
                    }
                    app.open_book(&p);
                }
            }
        }

        app
    }

    pub fn theme(&self) -> Theme {
        THEMES
            .get(self.state.theme_idx % THEMES.len())
            .or_else(|| THEMES.first())
            .copied()
            .unwrap_or(Theme {
                name: "Default",
                bg: ratatui::style::Color::Reset,
                fg: ratatui::style::Color::Reset,
                accent: ratatui::style::Color::Cyan,
                muted: ratatui::style::Color::DarkGray,
                status_bg: ratatui::style::Color::DarkGray,
                status_fg: ratatui::style::Color::White,
            })
    }

    pub fn esc_hold_remaining_secs(&self) -> Option<u64> {
        let start = self.esc_hold_start?;
        let hold_ms = self.config.esc_quit_hold_ms;
        let elapsed_ms = start.elapsed().as_millis() as u64;
        if elapsed_ms >= hold_ms {
            Some(0)
        } else {
            Some((hold_ms - elapsed_ms).div_ceil(1000))
        }
    }

    pub fn refresh_library(&mut self) {
        self.library.clear();
        let books_dir = self.config.resolved_library_dir();
        if !books_dir.exists() {
            let _ = std::fs::create_dir_all(&books_dir);
            let welcome = books_dir.join("Welcome.md");
            if !welcome.exists() {
                let _ = std::fs::write(
                    &welcome,
                    "# Welcome to zread\n\n\
                    A lightweight TUI e-reader built for the Cardputer Zero.\n\n\
                    Put `.epub`, `.md`, or `.txt` files into `~/Books` (configured in `~/.config/zread/config.json`).\n\n\
                    # Cardputer Controls\n\n\
                    Keypad `c` / `x` (Right / Down) or `Space` turn pages forward.\n\n\
                    Keypad `z` / `f` (Left / Up) turn pages backward.\n\n\
                    Press `t` for Table of Contents, `r` for RSVP speed-reading, `/` to search, `v` to switch themes, and hold `Esc` for 3 seconds to quit.\n",
                );
            }
        }

        for dir in self.config.resolved_scan_dirs() {
            self.scan_dir(&dir);
        }

        self.library
            .sort_by_key(|a| a.name.to_lowercase());
        if self.lib_selected >= self.library.len() {
            self.lib_selected = 0;
        }
    }

    fn scan_dir(&mut self, dir: &Path) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                continue;
            }
            let ext = path
                .extension()
                .and_then(|e| e.to_str())
                .map_or_default(str::to_ascii_lowercase);
            if matches!(ext.as_str(), "epub" | "txt" | "md") {
                if self.library.iter().any(|e| e.path == path) {
                    continue;
                }
                let name = path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("Untitled")
                    .to_string();
                let key = path.to_string_lossy().to_string();
                let has_progress = self.state.progress.contains_key(&key);
                let file_name = path
                    .file_name()
                    .and_then(|s| s.to_str())
                    .unwrap_or("");
                let protected = matches!(
                    file_name,
                    "Welcome.md" | "Cardputer_Zero_Field_Guide.epub"
                );
                self.library.push(LibraryEntry {
                    name,
                    ext: ext.to_uppercase(),
                    path,
                    has_progress,
                    protected,
                });
            }
        }
    }

    pub fn open_book(&mut self, path: &Path) {
        match Book::load(path) {
            Ok(book) => {
                let key = book.path.to_string_lossy().to_string();
                let had_saved = self.state.progress.contains_key(&key);
                let saved = self.state.get_progress(&book.path);
                let (w, h) = if self.last_size == (0, 0) {
                    (40, 14)
                } else {
                    self.last_size
                };
                self.book = Some(book);
                self.repaginate(
                    w,
                    h,
                    Some((saved.chapter, saved.para, saved.char_offset)),
                );
                self.mode = ScreenMode::Reader;
                self.status_msg = None;
                if self.last_size != (0, 0) && !had_saved {
                    self.save_current_progress();
                } else {
                    self.state.last_book = Some(key);
                    self.state.save();
                }
            }
            Err(e) => {
                self.status_msg = Some(e);
            }
        }
    }

    pub fn ensure_layout(&mut self, width: u16, height: u16) {
        if (width, height) != self.last_size {
            let was_uninitialized = self.last_size == (0, 0);
            self.last_size = (width, height);
            if let Some(book) = &self.book {
                let anchor = if was_uninitialized {
                    let saved = self.state.get_progress(&book.path);
                    Some((saved.chapter, saved.para, saved.char_offset))
                } else {
                    self.current_anchor()
                };
                self.repaginate(width, height, anchor);
            }
        }
    }

    fn current_anchor(&self) -> Option<(usize, usize, usize)> {
        let pag = self.paginated.as_ref()?;
        let page = pag.pages.get(self.current_page)?;
        Some((page.start_chapter, page.start_para, page.start_char))
    }

    pub fn margin_chars(&self) -> u16 {
        match self.config.margin {
            0 => 1,
            1 => 2,
            _ => 4,
        }
    }

    fn repaginate(&mut self, width: u16, height: u16, anchor: Option<(usize, usize, usize)>) {
        let Some(book) = &self.book else {
            return;
        };
        let margin = self.margin_chars();
        let text_w = width.saturating_sub(margin.saturating_mul(2)) as usize;
        let chrome_rows = if self.config.zen_mode { 0 } else { 2 };
        let text_h = height.saturating_sub(chrome_rows) as usize;
        let paginated = paginate_book(book, text_w, text_h, self.config.compact_paragraphs);
        let page_idx = anchor.map_or(0, |(ch, para, char_offset)| {
            find_page_for_position(&paginated.pages, ch, para, char_offset)
        });
        self.current_page = page_idx.min(paginated.pages.len().saturating_sub(1));
        self.paginated = Some(paginated);
    }

    pub fn save_current_progress(&mut self) {
        if let (Some(book), Some((ch, para, char_offset))) = (&self.book, self.current_anchor()) {
            self.state
                .set_progress(&book.path, ch, para, char_offset);
        }
        self.state.save();
    }

    pub fn rsvp_interval(&self) -> Duration {
        let wpm = u64::from(self.state.wpm.max(60));
        Duration::from_millis(60_000 / wpm)
    }

    pub fn tick(&mut self) -> bool {
        let mut redraw = false;

        if let Some(last) = self.esc_last_seen {
            if last.elapsed() > Duration::from_millis(650) {
                self.esc_hold_start = None;
                self.esc_last_seen = None;
                self.handle_esc_short();
                redraw = true;
            }
        }

        if let Some(rx) = &self.gut_search_rx {
            if let Ok(res) = rx.try_recv() {
                self.gut_busy = false;
                self.gut_search_rx = None;
                match res {
                    Ok(results) => {
                        self.gut_selected = 0;
                        self.gut_typing = results.is_empty();
                        self.gut_status = Some(if results.is_empty() {
                            "No results. Edit query & Enter".to_string()
                        } else {
                            format!("{} found · c/Enter:get · /:search", results.len())
                        });
                        self.gut_results = results;
                    }
                    Err(e) => {
                        self.gut_status = Some(e);
                    }
                }
                redraw = true;
            }
        }

        if let Some(rx) = &self.gut_dl_rx {
            if let Ok(res) = rx.try_recv() {
                self.gut_busy = false;
                self.gut_dl_rx = None;
                match res {
                    Ok(path) => {
                        self.refresh_library();
                        self.open_book(&path);
                    }
                    Err(e) => {
                        self.gut_status = Some(e);
                    }
                }
                redraw = true;
            }
        }

        if self.mode == ScreenMode::Rsvp
            && self.rsvp_playing
            && self.rsvp_last_tick.elapsed() >= self.rsvp_interval()
        {
            self.rsvp_last_tick = Instant::now();
            if self.rsvp_idx + 1 < self.rsvp_words.len() {
                self.rsvp_idx += 1;
            } else {
                self.rsvp_playing = false;
            }
            redraw = true;
        }

        redraw
    }

    pub fn handle_key(&mut self, key: KeyEvent) {
        if key.kind == KeyEventKind::Release {
            if key.code == KeyCode::Esc && self.esc_hold_start.is_some() {
                let held_long_enough = self
                    .esc_hold_start
                    .is_some_and(|s| s.elapsed() >= Duration::from_millis(self.config.esc_quit_hold_ms));
                self.esc_hold_start = None;
                self.esc_last_seen = None;
                if !held_long_enough {
                    self.handle_esc_short();
                }
            }
            return;
        }

        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            self.save_current_progress();
            self.should_quit = true;
            return;
        }

        if key.code == KeyCode::Esc {
            let now = Instant::now();
            let is_repeat = self
                .esc_last_seen
                .is_some_and(|t| now.duration_since(t) <= Duration::from_millis(650));

            if !is_repeat || self.esc_hold_start.is_none() {
                self.esc_hold_start = Some(now);
                self.esc_last_seen = Some(now);
            } else {
                self.esc_last_seen = Some(now);
                if let Some(start) = self.esc_hold_start {
                    if start.elapsed() >= Duration::from_millis(self.config.esc_quit_hold_ms) {
                        self.save_current_progress();
                        self.should_quit = true;
                    }
                }
            }
            return;
        }

        // Any other key cancels Esc hold.
        self.esc_hold_start = None;
        self.esc_last_seen = None;

        match self.mode {
            ScreenMode::Library => self.handle_library_key(key),
            ScreenMode::Reader => self.handle_reader_key(key),
            ScreenMode::Toc => self.handle_toc_key(key),
            ScreenMode::Bookmarks => self.handle_bookmarks_key(key),
            ScreenMode::Note => self.handle_note_key(key),
            ScreenMode::Search => self.handle_search_key(key),
            ScreenMode::Gutenberg => self.handle_gutenberg_key(key),
            ScreenMode::Help => self.handle_help_key(key),
            ScreenMode::Rsvp => self.handle_rsvp_key(key),
        }
    }

    fn handle_esc_short(&mut self) {
        match self.mode {
            ScreenMode::Library => {
                if self.lib_confirm_delete.take().is_some() {
                    return;
                }
                if self.book.is_some() {
                    self.mode = ScreenMode::Reader;
                }
            }
            ScreenMode::Gutenberg => {
                if self.gut_busy {
                    self.gut_busy = false;
                    self.gut_search_rx = None;
                    self.gut_dl_rx = None;
                    self.gut_typing = true;
                    self.gut_status = Some("Cancelled. Edit query & Enter".to_string());
                } else if !self.gut_typing {
                    self.gut_typing = true;
                    self.gut_status = Some("Type query & press Enter (Esc:Library)".to_string());
                } else {
                    self.mode = ScreenMode::Library;
                }
            }
            ScreenMode::Reader => {
                self.save_current_progress();
                self.refresh_library();
                self.mode = ScreenMode::Library;
            }
            ScreenMode::Toc
            | ScreenMode::Bookmarks
            | ScreenMode::Note
            | ScreenMode::Search
            | ScreenMode::Help => {
                self.mode = ScreenMode::Reader;
            }
            ScreenMode::Rsvp => {
                self.exit_rsvp();
            }
        }
    }

    fn handle_library_key(&mut self, key: KeyEvent) {
        if let Some(target) = self.lib_confirm_delete.clone() {
            match key.code {
                KeyCode::Char('y' | 'Y') | KeyCode::Enter => {
                    self.lib_confirm_delete = None;
                    self.delete_library_entry(&target);
                }
                KeyCode::Char('n' | 'N' | 'q') => {
                    self.lib_confirm_delete = None;
                }
                _ => {}
            }
            return;
        }

        match key.code {
            KeyCode::Char('q') => {
                self.state.save();
                self.should_quit = true;
            }
            KeyCode::Char('x' | 'j') | KeyCode::Down => {
                self.status_msg = None;
                if !self.library.is_empty() {
                    self.lib_selected = (self.lib_selected + 1) % self.library.len();
                }
            }
            KeyCode::Char('f' | 'k') | KeyCode::Up => {
                self.status_msg = None;
                if !self.library.is_empty() {
                    self.lib_selected = self
                        .lib_selected
                        .checked_sub(1)
                        .unwrap_or(self.library.len() - 1);
                }
            }
            KeyCode::Enter | KeyCode::Char(' ' | 'c' | '4') | KeyCode::Right => {
                if let Some(entry) = self.library.get(self.lib_selected).cloned() {
                    self.open_book(&entry.path);
                }
            }
            KeyCode::Char('d' | '6') | KeyCode::Backspace | KeyCode::Delete => {
                if let Some(entry) = self.library.get(self.lib_selected).cloned() {
                    if entry.protected {
                        self.status_msg = Some(" Help guide cannot be deleted".to_string());
                    } else {
                        self.lib_confirm_delete = Some(entry);
                    }
                }
            }
            KeyCode::Char('v' | '7') => {
                self.state.theme_idx = (self.state.theme_idx + 1) % THEMES.len();
                self.state.save();
            }
            KeyCode::Char('g' | '5') => {
                self.gut_query.clear();
                self.gut_typing = true;
                self.gut_results.clear();
                self.gut_selected = 0;
                self.gut_status = Some("Type query & press Enter".to_string());
                self.mode = ScreenMode::Gutenberg;
            }
            KeyCode::Char('R' | '8') => {
                self.refresh_library();
            }
            _ => {}
        }
    }

    fn delete_library_entry(&mut self, entry: &LibraryEntry) {
        if entry.protected {
            return;
        }
        match std::fs::remove_file(&entry.path) {
            Ok(()) => {
                let key = entry.path.to_string_lossy().to_string();
                self.state.progress.remove(&key);
                self.state.bookmarks.remove(&key);
                if self.state.last_book.as_deref() == Some(key.as_str()) {
                    self.state.last_book = None;
                }
                if self.book.as_ref().is_some_and(|b| b.path == entry.path) {
                    self.book = None;
                    self.paginated = None;
                }
                self.state.save();
                self.refresh_library();
                self.status_msg = Some(format!(" Deleted '{}'", entry.name));
            }
            Err(e) => {
                self.status_msg = Some(format!(" Delete failed: {e}"));
            }
        }
    }

    fn handle_reader_key(&mut self, key: KeyEvent) {
        let total_pages = self
            .paginated
            .as_ref()
            .map_or(1, |p| p.pages.len());

        match key.code {
            KeyCode::Char('q') => {
                self.save_current_progress();
                self.should_quit = true;
            }
            KeyCode::Char('l') => {
                self.save_current_progress();
                self.refresh_library();
                self.mode = ScreenMode::Library;
            }
            // Down / Right -> Forward
            KeyCode::Char(' ' | 'x' | 'c' | 'j' | 'n') | KeyCode::Down | KeyCode::Right |
KeyCode::PageDown => {
                if self.current_page + 1 < total_pages {
                    self.current_page += 1;
                    self.state.pages_read = self.state.pages_read.saturating_add(1);
                    self.save_current_progress();
                }
            }
            // Up / Left -> Backward
            KeyCode::Char('f' | 'z' | 'k' | 'p') | KeyCode::Up | KeyCode::Left |
KeyCode::PageUp => {
                if self.current_page > 0 {
                    self.current_page -= 1;
                    self.save_current_progress();
                }
            }
            KeyCode::Char(']') => {
                self.jump_chapter(1);
                self.save_current_progress();
            }
            KeyCode::Char('[') => {
                self.jump_chapter(-1);
                self.save_current_progress();
            }
            KeyCode::Char('g') | KeyCode::Home => {
                self.current_page = 0;
                self.save_current_progress();
            }
            KeyCode::Char('G') | KeyCode::End => {
                self.current_page = total_pages.saturating_sub(1);
                self.save_current_progress();
            }
            KeyCode::Char('v') => {
                self.state.theme_idx = (self.state.theme_idx + 1) % THEMES.len();
                self.state.save();
            }
            KeyCode::Char('M') => {
                self.config.margin = (self.config.margin + 1) % 3;
                self.config.save();
                let anchor = self.current_anchor();
                self.repaginate(self.last_size.0, self.last_size.1, anchor);
                self.save_current_progress();
            }
            KeyCode::Char('P') => {
                self.config.compact_paragraphs = !self.config.compact_paragraphs;
                self.config.save();
                let anchor = self.current_anchor();
                self.repaginate(self.last_size.0, self.last_size.1, anchor);
                self.save_current_progress();
            }
            KeyCode::Char('H') => {
                self.config.zen_mode = !self.config.zen_mode;
                self.config.save();
                let anchor = self.current_anchor();
                self.repaginate(self.last_size.0, self.last_size.1, anchor);
                self.save_current_progress();
            }
            KeyCode::Char('t' | '4') => {
                if let Some((ch, _, _)) = self.current_anchor() {
                    self.toc_selected = ch;
                }
                self.mode = ScreenMode::Toc;
            }
            KeyCode::Char('m' | '5') => {
                self.add_current_bookmark();
            }
            KeyCode::Char('N') => {
                self.note_buf.clear();
                self.mode = ScreenMode::Note;
            }
            KeyCode::Char('b') => {
                self.bm_selected = 0;
                self.mode = ScreenMode::Bookmarks;
            }
            KeyCode::Char('/' | '6') => {
                self.search_query.clear();
                self.search_matches.clear();
                self.search_selected = 0;
                self.mode = ScreenMode::Search;
            }
            KeyCode::Char('?' | '8') => {
                self.help_scroll = 0;
                self.mode = ScreenMode::Help;
            }
            KeyCode::Char('r' | '7') => {
                self.enter_rsvp();
            }
            _ => {}
        }
    }

    fn jump_chapter(&mut self, delta: isize) {
        let Some(pag) = &self.paginated else {
            return;
        };
        let cur_ch = pag
            .pages
            .get(self.current_page)
            .map_or(0, |p| p.start_chapter);
        let target = (cur_ch as isize + delta)
            .clamp(0, pag.chapter_start_pages.len().saturating_sub(1) as isize)
            as usize;
        if let Some(&page) = pag.chapter_start_pages.get(target) {
            self.current_page = page;
        }
    }

    fn add_current_bookmark(&mut self) {
        let (Some(book), Some(pag), Some((ch, para, _))) =
            (&self.book, &self.paginated, self.current_anchor())
        else {
            return;
        };
        let snippet = pag
            .pages
            .get(self.current_page)
            .and_then(|p| p.lines.iter().find(|l| !l.trim().is_empty())).map_or_else(|| format!("Ch {}", ch + 1), |l| l.chars().take(28).collect::<String>());
        let label = format!("Ch {} · {}", ch + 1, snippet);
        self.state.add_bookmark(&book.path, ch, para, label);
        self.state.save();
        self.status_msg = Some("Bookmarked!".to_string());
    }

    fn handle_bookmarks_key(&mut self, key: KeyEvent) {
        let count = self
            .book
            .as_ref()
            .map_or(0, |b| self.state.get_bookmarks(&b.path).len());

        match key.code {
            KeyCode::Char('b' | 'q' | 'z') | KeyCode::Left => {
                self.mode = ScreenMode::Reader;
            }
            KeyCode::Char('x' | 'j') | KeyCode::Down => {
                if count > 0 {
                    self.bm_selected = (self.bm_selected + 1) % count;
                }
            }
            KeyCode::Char('f' | 'k') | KeyCode::Up => {
                if count > 0 {
                    self.bm_selected = self.bm_selected.checked_sub(1).unwrap_or(count - 1);
                }
            }
            KeyCode::Char('m') => {
                self.add_current_bookmark();
            }
            KeyCode::Char('d') | KeyCode::Backspace | KeyCode::Delete => {
                if let Some(book) = &self.book {
                    self.state.remove_bookmark(&book.path, self.bm_selected);
                    self.state.save();
                    let new_count = self.state.get_bookmarks(&book.path).len();
                    if new_count == 0 {
                        self.bm_selected = 0;
                    } else if self.bm_selected >= new_count {
                        self.bm_selected = new_count - 1;
                    }
                }
            }
            KeyCode::Enter | KeyCode::Char(' ' | 'c') | KeyCode::Right => {
                if let (Some(book), Some(pag)) = (&self.book, &self.paginated) {
                    if let Some(bm) = self.state.get_bookmarks(&book.path).get(self.bm_selected) {
                        self.current_page =
                            find_page_for_position(&pag.pages, bm.chapter, bm.para, 0);
                    }
                }
                self.mode = ScreenMode::Reader;
                self.save_current_progress();
            }
            _ => {}
        }
    }

    fn handle_gutenberg_key(&mut self, key: KeyEvent) {
        if self.gut_busy {
            return;
        }
        if self.gut_typing {
            match key.code {
                KeyCode::Enter => {
                    let query = self.gut_query.clone();
                    if query.trim().is_empty() {
                        return;
                    }
                    self.gut_busy = true;
                    self.gut_typing = false;
                    self.gut_results.clear();
                    self.gut_status = Some("Searching (Esc to cancel)...".to_string());
                    let (tx, rx) = mpsc::channel();
                    self.gut_search_rx = Some(rx);
                    std::thread::spawn(move || {
                        let _ = tx.send(search_gutenberg(&query));
                    });
                }
                KeyCode::Backspace => {
                    self.gut_query.pop();
                }
                KeyCode::Char(c) => {
                    self.gut_query.push(c);
                }
                _ => {}
            }
            return;
        }

        match key.code {
            KeyCode::Char('/') => {
                self.gut_typing = true;
                self.gut_status = Some("Type query & press Enter".to_string());
            }
            KeyCode::Char('x' | 'j') | KeyCode::Down => {
                if !self.gut_results.is_empty() {
                    self.gut_selected = (self.gut_selected + 1) % self.gut_results.len();
                }
            }
            KeyCode::Char('f' | 'k') | KeyCode::Up => {
                if !self.gut_results.is_empty() {
                    self.gut_selected = self
                        .gut_selected
                        .checked_sub(1)
                        .unwrap_or(self.gut_results.len() - 1);
                }
            }
            KeyCode::Enter | KeyCode::Char(' ' | 'c') | KeyCode::Right => {
                if let Some(book) = self.gut_results.get(self.gut_selected).cloned() {
                    let lib_dir = self.config.resolved_library_dir();
                    self.gut_busy = true;
                    self.gut_status = Some("Downloading EPUB...".to_string());
                    let (tx, rx) = mpsc::channel();
                    self.gut_dl_rx = Some(rx);
                    std::thread::spawn(move || {
                        let _ = tx.send(download_book(&book, &lib_dir));
                    });
                }
            }
            KeyCode::Char('q') => {
                self.mode = ScreenMode::Library;
            }
            _ => {}
        }
    }

    fn handle_note_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Enter => {
                self.save_current_note();
                self.mode = ScreenMode::Reader;
            }
            KeyCode::Backspace => {
                self.note_buf.pop();
            }
            KeyCode::Char(c) => {
                self.note_buf.push(c);
            }
            _ => {}
        }
    }

    fn save_current_note(&mut self) {
        use std::io::Write;
        let text = self.note_buf.trim();
        if text.is_empty() {
            return;
        }
        let Some(book) = &self.book else {
            return;
        };
        let notes_dir = self.config.resolved_library_dir().join("Notes");
        let _ = std::fs::create_dir_all(&notes_dir);
        let safe_stem = book
            .path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("Book");
        let note_path = notes_dir.join(format!("{safe_stem}.md"));
        let (ch, _, _) = self.current_anchor().unwrap_or((0, 0, 0));
        let total = self.paginated.as_ref().map_or(1, |p| p.pages.len().max(1));
        let pct = ((self.current_page + 1) * 100) / total;

        let needs_header = !note_path.exists();
        if let Ok(mut file) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&note_path)
        {
            if needs_header {
                let _ = writeln!(file, "# Notes: {}\n", book.title);
            }
            let _ = writeln!(file, "- **Ch {} ({pct}%):** {text}", ch + 1);
            self.status_msg = Some("Saved note!".to_string());
        }
    }

    fn handle_toc_key(&mut self, key: KeyEvent) {
        let ch_count = self.book.as_ref().map_or(0, |b| b.chapters.len());
        match key.code {
            KeyCode::Char('t' | 'q' | 'z') | KeyCode::Left => {
                self.mode = ScreenMode::Reader;
            }
            KeyCode::Char('x' | 'j') | KeyCode::Down => {
                if ch_count > 0 {
                    self.toc_selected = (self.toc_selected + 1) % ch_count;
                }
            }
            KeyCode::Char('f' | 'k') | KeyCode::Up => {
                if ch_count > 0 {
                    self.toc_selected = self.toc_selected.checked_sub(1).unwrap_or(ch_count - 1);
                }
            }
            KeyCode::Enter | KeyCode::Char(' ' | 'c') | KeyCode::Right => {
                if let Some(pag) = &self.paginated {
                    if let Some(&page) = pag.chapter_start_pages.get(self.toc_selected) {
                        self.current_page = page;
                    }
                }
                self.mode = ScreenMode::Reader;
                self.save_current_progress();
            }
            _ => {}
        }
    }

    fn handle_search_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Enter => {
                if let Some((page, _)) = self.search_matches.get(self.search_selected) {
                    self.current_page = *page;
                    self.mode = ScreenMode::Reader;
                    self.save_current_progress();
                } else {
                    self.run_search();
                }
            }
            KeyCode::Down => {
                if !self.search_matches.is_empty() {
                    self.search_selected = (self.search_selected + 1) % self.search_matches.len();
                }
            }
            KeyCode::Up => {
                if !self.search_matches.is_empty() {
                    self.search_selected = self
                        .search_selected
                        .checked_sub(1)
                        .unwrap_or(self.search_matches.len() - 1);
                }
            }
            KeyCode::Backspace => {
                self.search_query.pop();
                self.run_search();
            }
            KeyCode::Char(c) => {
                self.search_query.push(c);
                self.run_search();
            }
            _ => {}
        }
    }

    fn run_search(&mut self) {
        self.search_matches.clear();
        self.search_selected = 0;
        let q = self.search_query.trim().to_ascii_lowercase();
        if q.len() < 2 {
            return;
        }
        let Some(pag) = &self.paginated else {
            return;
        };
        for (page_idx, page) in pag.pages.iter().enumerate() {
            for line in &page.lines {
                if line.to_ascii_lowercase().contains(&q) {
                    self.search_matches.push((page_idx, line.clone()));
                    break;
                }
            }
            if self.search_matches.len() >= 30 {
                break;
            }
        }
    }

    fn handle_help_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char('?' | 'q' | ' ') => {
                self.mode = ScreenMode::Reader;
            }
            KeyCode::Char('x' | 'j') | KeyCode::Down => {
                self.help_scroll = self.help_scroll.saturating_add(1).min(20);
            }
            KeyCode::Char('f' | 'k') | KeyCode::Up => {
                self.help_scroll = self.help_scroll.saturating_sub(1);
            }
            _ => {}
        }
    }

    fn enter_rsvp(&mut self) {
        let Some(book) = &self.book else {
            return;
        };
        let (start_ch, start_para, _) = self.current_anchor().unwrap_or((0, 0, 0));
        self.rsvp_words.clear();
        let mut start_word_idx = 0usize;
        let mut found_start = false;

        for (ch_idx, chapter) in book.chapters.iter().enumerate() {
            for (p_idx, para) in chapter.paragraphs.iter().enumerate() {
                if !found_start && (ch_idx, p_idx) >= (start_ch, start_para) {
                    start_word_idx = self.rsvp_words.len();
                    found_start = true;
                }
                for word in para.split_whitespace() {
                    self.rsvp_words.push((word.to_string(), ch_idx, p_idx));
                }
            }
        }

        if self.rsvp_words.is_empty() {
            return;
        }
        self.rsvp_idx = start_word_idx.min(self.rsvp_words.len() - 1);
        self.rsvp_playing = true;
        self.rsvp_last_tick = Instant::now();
        self.mode = ScreenMode::Rsvp;
    }

    fn exit_rsvp(&mut self) {
        self.rsvp_playing = false;
        if let (Some(&(_, ch, para)), Some(pag)) =
            (self.rsvp_words.get(self.rsvp_idx), &self.paginated)
        {
            self.current_page = find_page_for_position(&pag.pages, ch, para, 0);
        }
        self.mode = ScreenMode::Reader;
        self.save_current_progress();
    }

    fn handle_rsvp_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char('r' | 'q') => self.exit_rsvp(),
            KeyCode::Char(' ') => {
                self.rsvp_playing = !self.rsvp_playing;
                self.rsvp_last_tick = Instant::now();
            }
            KeyCode::Char('+' | '=' | 'f') | KeyCode::Up => {
                self.state.wpm = (self.state.wpm + 20).min(800);
            }
            KeyCode::Char('-' | 'x') | KeyCode::Down => {
                self.state.wpm = self.state.wpm.saturating_sub(20).max(80);
            }
            KeyCode::Left | KeyCode::Char('z' | 'b') => {
                self.rsvp_idx = self.rsvp_idx.saturating_sub(10);
            }
            KeyCode::Right | KeyCode::Char('c' | 'n') => {
                self.rsvp_idx = (self.rsvp_idx + 10).min(self.rsvp_words.len().saturating_sub(1));
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::book::{Book, Chapter};

    fn test_app_with_book() -> App {
        let mut app = App::new(None);
        let book = Book {
            title: "Test Book".to_string(),
            path: PathBuf::from("/tmp/zread_test_book.epub"),
            chapters: vec![
                Chapter {
                    title: "Chapter One".to_string(),
                    paragraphs: (0..20)
                        .map(|i| format!("Paragraph {i} with enough words to wrap across lines."))
                        .collect(),
                },
                Chapter {
                    title: "Chapter Two".to_string(),
                    paragraphs: vec!["Second chapter text.".to_string()],
                },
            ],
        };
        app.book = Some(book);
        app.last_size = (40, 12);
        app.repaginate(40, 12, None);
        app.mode = ScreenMode::Reader;
        app
    }

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn release_esc(code: KeyCode) -> KeyEvent {
        KeyEvent {
            code,
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Release,
            state: crossterm::event::KeyEventState::NONE,
        }
    }

    #[test]
    fn keypad_navigation_and_overlays() {
        let mut app = test_app_with_book();
        assert_eq!(app.current_page, 0);

        // c / x move forward; z / f move backward
        app.handle_key(key(KeyCode::Char('c')));
        assert_eq!(app.current_page, 1);
        app.handle_key(key(KeyCode::Char('x')));
        assert_eq!(app.current_page, 2);
        app.handle_key(key(KeyCode::Char('z')));
        assert_eq!(app.current_page, 1);
        app.handle_key(key(KeyCode::Char('f')));
        assert_eq!(app.current_page, 0);

        // t opens Table of Contents, Esc release closes it
        app.handle_key(key(KeyCode::Char('t')));
        assert_eq!(app.mode, ScreenMode::Toc);
        app.handle_key(key(KeyCode::Esc));
        assert_eq!(app.mode, ScreenMode::Toc);
        app.handle_key(release_esc(KeyCode::Esc));
        assert_eq!(app.mode, ScreenMode::Reader);

        // l opens Library, Esc release returns to open book
        app.handle_key(key(KeyCode::Char('l')));
        assert_eq!(app.mode, ScreenMode::Library);
        app.handle_key(key(KeyCode::Esc));
        app.handle_key(release_esc(KeyCode::Esc));
        assert_eq!(app.mode, ScreenMode::Reader);
    }

    #[test]
    fn bookmarks_add_and_delete() {
        let mut app = test_app_with_book();
        app.handle_key(key(KeyCode::Char('c')));
        app.handle_key(key(KeyCode::Char('m')));
        app.handle_key(key(KeyCode::Char('b')));
        assert_eq!(app.mode, ScreenMode::Bookmarks);
        assert_eq!(
            app.book
                .as_ref()
                .map_or(0, |b| app.state.get_bookmarks(&b.path).len()),
            1
        );

        app.handle_key(key(KeyCode::Char('d')));
        assert_eq!(
            app.book
                .as_ref()
                .map_or(1, |b| app.state.get_bookmarks(&b.path).len()),
            0
        );
        app.handle_key(key(KeyCode::Esc));
        app.handle_key(release_esc(KeyCode::Esc));
        assert_eq!(app.mode, ScreenMode::Reader);
    }

    #[test]
    fn esc_hold_three_seconds_quits() {
        let mut app = test_app_with_book();
        app.handle_key(key(KeyCode::Esc));
        assert!(!app.should_quit);
        assert_eq!(app.mode, ScreenMode::Reader);

        // Simulate Esc held for 3.1 seconds: quits without changing mode first
        app.esc_hold_start = Instant::now().checked_sub(Duration::from_millis(3100));
        app.esc_last_seen = Some(Instant::now());
        app.handle_key(key(KeyCode::Esc));
        assert!(app.should_quit);
        assert_eq!(app.mode, ScreenMode::Reader);
    }

    #[test]
    fn delete_confirmation_and_protected_guard() {
        let mut app = App::new(None);
        app.mode = ScreenMode::Library;
        app.library = vec![
            LibraryEntry {
                name: "Welcome".to_string(),
                ext: "MD".to_string(),
                path: PathBuf::from("/tmp/zread_nonexistent_welcome.md"),
                has_progress: false,
                protected: true,
            },
            LibraryEntry {
                name: "Removable".to_string(),
                ext: "EPUB".to_string(),
                path: PathBuf::from("/tmp/zread_nonexistent_book.epub"),
                has_progress: false,
                protected: false,
            },
        ];
        app.lib_selected = 0;
        app.handle_key(key(KeyCode::Char('d')));
        assert!(app.lib_confirm_delete.is_none());

        app.lib_selected = 1;
        app.handle_key(key(KeyCode::Char('d')));
        assert!(app.lib_confirm_delete.is_some());

        app.handle_key(key(KeyCode::Char('n')));
        assert!(app.lib_confirm_delete.is_none());
    }

    #[test]
    fn search_rsvp_chapters_margins_and_help_scroll() {
        let mut app = test_app_with_book();
        // Chapter jumps & start/end
        app.handle_key(key(KeyCode::Char(']')));
        assert!(app.current_page > 0);
        app.handle_key(key(KeyCode::Char('[')));
        assert_eq!(app.current_page, 0);
        app.handle_key(key(KeyCode::Char('G')));
        assert!(app.current_page > 0);
        app.handle_key(key(KeyCode::Char('g')));
        assert_eq!(app.current_page, 0);

        // Search in book
        app.handle_key(key(KeyCode::Char('/')));
        assert_eq!(app.mode, ScreenMode::Search);
        for ch in "Second".chars() {
            app.handle_key(key(KeyCode::Char(ch)));
        }
        assert_eq!(app.search_matches.len(), 1);
        app.handle_key(key(KeyCode::Enter));
        assert_eq!(app.mode, ScreenMode::Reader);
        assert!(app.current_page > 0);

        // RSVP mode
        app.handle_key(key(KeyCode::Char('r')));
        assert_eq!(app.mode, ScreenMode::Rsvp);
        let orig_wpm = app.state.wpm;
        app.handle_key(key(KeyCode::Char('f')));
        assert_eq!(app.state.wpm, orig_wpm + 20);
        app.handle_key(key(KeyCode::Char('x')));
        assert_eq!(app.state.wpm, orig_wpm);
        app.handle_key(key(KeyCode::Char('r')));
        assert_eq!(app.mode, ScreenMode::Reader);

        // Margins, compact paragraphs, and help scroll
        let old_margin = app.config.margin;
        app.handle_key(key(KeyCode::Char('M')));
        assert_ne!(app.config.margin, old_margin);
        let old_compact = app.config.compact_paragraphs;
        app.handle_key(key(KeyCode::Char('P')));
        assert_ne!(app.config.compact_paragraphs, old_compact);
        let old_zen = app.config.zen_mode;
        app.handle_key(key(KeyCode::Char('H')));
        assert_ne!(app.config.zen_mode, old_zen);

        app.handle_key(key(KeyCode::Char('?')));
        assert_eq!(app.mode, ScreenMode::Help);
        app.handle_key(key(KeyCode::Char('x')));
        assert_eq!(app.help_scroll, 1);
        app.handle_key(key(KeyCode::Char('f')));
        assert_eq!(app.help_scroll, 0);
        app.handle_key(key(KeyCode::Char('?')));
        assert_eq!(app.mode, ScreenMode::Reader);
    }

    #[test]
    fn quick_note_input_and_cancel() {
        let mut app = test_app_with_book();
        app.handle_key(key(KeyCode::Char('N')));
        assert_eq!(app.mode, ScreenMode::Note);
        for ch in "Good quote".chars() {
            app.handle_key(key(KeyCode::Char(ch)));
        }
        assert_eq!(app.note_buf, "Good quote");
        app.handle_key(key(KeyCode::Backspace));
        assert_eq!(app.note_buf, "Good quot");
        app.handle_key(key(KeyCode::Esc));
        app.handle_key(release_esc(KeyCode::Esc));
        assert_eq!(app.mode, ScreenMode::Reader);
    }

    #[test]
    fn ensure_layout_preserves_saved_sub_paragraph_position() {
        let mut app = App::new(None);
        let long_para = (0..120)
            .map(|i| format!("token{i:02}"))
            .collect::<Vec<_>>()
            .join(" ");
        let book = Book {
            title: "Resume Test".to_string(),
            path: PathBuf::from("/tmp/zread_resume_test.epub"),
            chapters: vec![Chapter {
                title: "C1".to_string(),
                paragraphs: vec![long_para],
            }],
        };
        // Paginate at target device size (40x10) and pick page 3's exact anchor.
        app.book = Some(book);
        app.repaginate(40, 10, None);
        app.current_page = 3;
        let expected_anchor = app.current_anchor().unwrap_or((0, 0, 0));
        assert!(expected_anchor.2 > 0);
        let book_path = app
            .book
            .as_ref()
            .map_or_default(|b| b.path.clone());
        app.state
            .set_progress(&book_path, expected_anchor.0, expected_anchor.1, expected_anchor.2);

        // Simulate fresh launch before terminal size is measured: last_size == (0, 0).
        app.last_size = (0, 0);
        app.repaginate(40, 14, Some(expected_anchor));
        app.ensure_layout(40, 10);
        assert_eq!(app.current_page, 3);
        assert_eq!(app.current_anchor(), Some(expected_anchor));
    }
}
