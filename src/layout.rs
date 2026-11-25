use crate::book::Book;
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

#[derive(Debug, Clone)]
pub struct RenderedLine {
    pub text: String,
    pub chapter_idx: usize,
    pub para_idx: usize,
}

#[derive(Debug, Clone)]
pub struct Page {
    pub lines: Vec<String>,
    pub start_chapter: usize,
    pub start_para: usize,
}

#[derive(Debug, Clone)]
pub struct PaginatedBook {
    pub pages: Vec<Page>,
    pub chapter_start_pages: Vec<usize>,
}

pub fn paginate_book(
    book: &Book,
    width: usize,
    height: usize,
    compact_paragraphs: bool,
) -> PaginatedBook {
    let wrap_width = width.max(12);
    let page_height = height.max(3);

    let mut all_lines: Vec<RenderedLine> = Vec::new();
    let mut chapter_first_line: Vec<usize> = Vec::with_capacity(book.chapters.len());

    for (ch_idx, chapter) in book.chapters.iter().enumerate() {
        chapter_first_line.push(all_lines.len());
        for (p_idx, para) in chapter.paragraphs.iter().enumerate() {
            let formatted;
            let text_to_wrap = if compact_paragraphs && p_idx > 0 {
                formatted = format!("  {para}");
                formatted.as_str()
            } else {
                para.as_str()
            };
            for wrapped in wrap_paragraph(text_to_wrap, wrap_width) {
                all_lines.push(RenderedLine {
                    text: wrapped,
                    chapter_idx: ch_idx,
                    para_idx: p_idx,
                });
            }
            if !compact_paragraphs {
                all_lines.push(RenderedLine {
                    text: String::new(),
                    chapter_idx: ch_idx,
                    para_idx: p_idx,
                });
            }
        }
    }

    if all_lines.is_empty() {
        all_lines.push(RenderedLine {
            text: "(Empty book)".to_string(),
            chapter_idx: 0,
            para_idx: 0,
        });
    }

    let mut pages = Vec::new();
    let mut idx = 0;
    while let Some(first) = all_lines.get(idx) {
        if first.text.is_empty() {
            idx += 1;
            continue;
        }
        let end = (idx + page_height).min(all_lines.len());
        let slice = all_lines.get(idx..end).unwrap_or_default();
        let lines = slice.iter().map(|l| l.text.clone()).collect();
        pages.push(Page {
            lines,
            start_chapter: first.chapter_idx,
            start_para: first.para_idx,
        });
        idx = end;
    }

    if pages.is_empty() {
        pages.push(Page {
            lines: vec!["(Empty book)".to_string()],
            start_chapter: 0,
            start_para: 0,
        });
    }

    let chapter_start_pages = chapter_first_line
        .iter()
        .enumerate()
        .map(|(ch_idx, _)| {
            pages
                .iter()
                .position(|p| p.start_chapter >= ch_idx)
                .unwrap_or(pages.len().saturating_sub(1))
        })
        .collect();

    PaginatedBook {
        pages,
        chapter_start_pages,
    }
}

pub fn find_page_for_position(pages: &[Page], chapter: usize, para: usize) -> usize {
    pages
        .iter()
        .position(|p| (p.start_chapter, p.start_para) >= (chapter, para))
        .unwrap_or_else(|| pages.len().saturating_sub(1))
}

fn wrap_paragraph(para: &str, width: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let mut current = String::new();
    let mut current_len = 0usize;
    let leading_indent = para.starts_with("  ");

    for raw_word in para.split_whitespace() {
        if current.is_empty() && lines.is_empty() && leading_indent && width > 4 {
            current.push_str("  ");
            current_len = 2;
        }
        // Pala One-style breakable punctuation splitting (hyphens, slashes, etc.).
        let pieces = split_after_breakable(raw_word);
        for (piece_idx, word) in pieces.iter().enumerate() {
            let need_space = piece_idx == 0 && !current.is_empty() && current != "  ";
            let word_len = UnicodeWidthStr::width(*word);
            let extra = usize::from(need_space);

            if current.is_empty() {
                if word_len <= width {
                    current.push_str(word);
                    current_len = word_len;
                } else {
                    push_hard_wrapped(word, width, &mut lines, &mut current, &mut current_len);
                }
            } else if current_len + extra + word_len <= width {
                if need_space {
                    current.push(' ');
                }
                current.push_str(word);
                current_len += extra + word_len;
            } else {
                lines.push(std::mem::take(&mut current));
                current_len = 0;
                if word_len <= width {
                    current.push_str(word);
                    current_len = word_len;
                } else {
                    push_hard_wrapped(word, width, &mut lines, &mut current, &mut current_len);
                }
            }
        }
    }

    if !current.is_empty() {
        lines.push(current);
    }
    lines
}

fn is_breakable_punct(ch: char) -> bool {
    matches!(
        ch,
        '-' | '/' | '.' | ',' | ';' | ':' | '!' | '?' | ')' | ']' | '}'
    )
}

fn split_after_breakable(word: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut start = 0;
    for (idx, ch) in word.char_indices() {
        let next = idx + ch.len_utf8();
        if is_breakable_punct(ch) && next < word.len() {
            if let Some(seg) = word.get(start..next) {
                parts.push(seg);
                start = next;
            }
        }
    }
    if let Some(rest) = word.get(start..) {
        if !rest.is_empty() {
            parts.push(rest);
        }
    }
    if parts.is_empty() {
        parts.push(word);
    }
    parts
}

fn push_hard_wrapped(
    word: &str,
    width: usize,
    lines: &mut Vec<String>,
    current: &mut String,
    current_len: &mut usize,
) {
    let mut chunk = String::new();
    let mut len = 0;
    for ch in word.chars() {
        let cw = ch.width().unwrap_or(0);
        if len > 0 && len + cw > width {
            lines.push(std::mem::take(&mut chunk));
            len = 0;
        }
        chunk.push(ch);
        len += cw;
        if len >= width {
            lines.push(std::mem::take(&mut chunk));
            len = 0;
        }
    }
    if !chunk.is_empty() {
        *current = chunk;
        *current_len = len;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::book::Chapter;
    use std::path::PathBuf;

    #[test]
    fn hyphen_and_slash_wrapping_and_hard_wrap() {
        let wrapped = wrap_paragraph("twenty-seven-year-old supercalifragilistic end-", 14);
        assert_eq!(
            wrapped,
            vec![
                "twenty-seven-",
                "year-old",
                "supercalifragi",
                "listic end-",
            ]
        );
        // Slash-separated URLs/tokens break cleanly after '/'
        assert_eq!(
            wrap_paragraph("https://cardputer.cc/docs", 16),
            vec!["https://", "cardputer.cc/", "docs"]
        );
        // Exact-width hard-wrapped word (no remainder)
        assert_eq!(wrap_paragraph("abcd", 2), vec!["ab", "cd"]);
    }

    #[test]
    fn paginate_compact_vs_relaxed_and_position_lookup() {
        let book = Book {
            title: "T".to_string(),
            path: PathBuf::from("t.txt"),
            chapters: vec![
                Chapter {
                    title: "C1".to_string(),
                    paragraphs: vec![
                        "One".to_string(),
                        "Two".to_string(),
                        "Three".to_string(),
                        "ThreeB".to_string(),
                    ],
                },
                Chapter {
                    title: "C2".to_string(),
                    paragraphs: vec!["Four".to_string()],
                },
            ],
        };
        let relaxed = paginate_book(&book, 20, 3, false);
        let compact = paginate_book(&book, 20, 3, true);
        assert!(compact.pages.len() < relaxed.pages.len());
        assert_eq!(compact.chapter_start_pages.len(), 2);
        // Compact mode indents subsequent paragraphs by 2 spaces
        assert_eq!(
            compact.pages.first().and_then(|p| p.lines.get(1)).map(String::as_str),
            Some("  Two")
        );
        // Relaxed has leading blank lines skipped at page boundaries
        assert!(relaxed.pages.iter().all(|p| p.lines.first().is_some_and(|l| !l.is_empty())));
        let c2_page = find_page_for_position(&relaxed.pages, 1, 0);
        assert_eq!(relaxed.pages.get(c2_page).map(|p| p.start_chapter), Some(1));
    }
}
