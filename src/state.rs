use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BookProgress {
    pub chapter: usize,
    pub para: usize,
    #[serde(default)]
    pub char_offset: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Bookmark {
    pub chapter: usize,
    pub para: usize,
    pub label: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppState {
    pub theme_idx: usize,
    pub wpm: u16,
    #[serde(default)]
    pub pages_read: u64,
    pub last_book: Option<String>,
    pub progress: HashMap<String, BookProgress>,
    #[serde(default)]
    pub bookmarks: HashMap<String, Vec<Bookmark>>,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            theme_idx: 0,
            wpm: 260,
            pages_read: 0,
            last_book: None,
            progress: HashMap::new(),
            bookmarks: HashMap::new(),
        }
    }
}

impl AppState {
    pub fn state_path() -> PathBuf {
        let home = std::env::var_os("HOME").map_or_else(|| PathBuf::from("."), PathBuf::from);
        home.join(".local/share/zread/state.json")
    }

    pub fn load() -> Self {
        let path = Self::state_path();
        std::fs::read_to_string(&path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) {
        let path = Self::state_path();
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(json) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(&path, json);
        }
    }

    pub fn get_progress(&self, book_path: &Path) -> BookProgress {
        let key = book_path.to_string_lossy().to_string();
        self.progress.get(&key).cloned().unwrap_or_default()
    }

    pub fn set_progress(
        &mut self,
        book_path: &Path,
        chapter: usize,
        para: usize,
        char_offset: usize,
    ) {
        let key = book_path.to_string_lossy().to_string();
        self.last_book = Some(key.clone());
        self.progress.insert(
            key,
            BookProgress {
                chapter,
                para,
                char_offset,
            },
        );
    }

    pub fn get_bookmarks(&self, book_path: &Path) -> &[Bookmark] {
        let key = book_path.to_string_lossy().to_string();
        self.bookmarks.get(&key).map_or(&[], Vec::as_slice)
    }

    pub fn add_bookmark(&mut self, book_path: &Path, chapter: usize, para: usize, label: String) {
        let key = book_path.to_string_lossy().to_string();
        let list = self.bookmarks.entry(key).or_default();
        if !list.iter().any(|b| b.chapter == chapter && b.para == para) {
            list.push(Bookmark {
                chapter,
                para,
                label,
            });
        }
    }

    pub fn remove_bookmark(&mut self, book_path: &Path, idx: usize) {
        let key = book_path.to_string_lossy().to_string();
        if let Some(list) = self.bookmarks.get_mut(&key) {
            if idx < list.len() {
                list.remove(idx);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn progress_and_bookmark_dedup_and_bounds() {
        let mut state = AppState::default();
        let path = PathBuf::from("/tmp/sample.epub");
        state.set_progress(&path, 2, 7, 42);
        let prog = state.get_progress(&path);
        assert_eq!((prog.chapter, prog.para, prog.char_offset), (2, 7, 42));

        let legacy: BookProgress = serde_json::from_str(r#"{"chapter":1,"para":5}"#).unwrap_or_default();
        assert_eq!((legacy.chapter, legacy.para, legacy.char_offset), (1, 5, 0));

        state.add_bookmark(&path, 1, 3, "Ch 2".to_string());
        state.add_bookmark(&path, 1, 3, "Duplicate".to_string());
        state.add_bookmark(&path, 1, 4, "Different para".to_string());
        assert_eq!(state.get_bookmarks(&path).len(), 2);

        state.remove_bookmark(&path, 2); // exact len boundary
        assert_eq!(state.get_bookmarks(&path).len(), 2);
        state.remove_bookmark(&path, 1);
        state.remove_bookmark(&path, 0);
        assert!(state.get_bookmarks(&path).is_empty());
        assert!(AppState::state_path().ends_with("zread/state.json"));
    }
}
