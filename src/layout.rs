use crate::book::Book;
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

#[derive(Debug, Clone)]
pub struct RenderedLine {
    pub text: String,
    pub chapter_idx: usize,
    pub para_idx: usize,
    pub char_offset: usize,
}

#[derive(Debug, Clone)]
pub struct Page {
    pub lines: Vec<String>,
    pub start_chapter: usize,
    pub start_para: usize,
    pub start_char: usize,
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
            let indent = compact_paragraphs && p_idx > 0;
            for (wrapped, char_offset) in wrap_paragraph(para, wrap_width, indent) {
                all_lines.push(RenderedLine {
                    text: wrapped,
                    chapter_idx: ch_idx,
                    para_idx: p_idx,
                    char_offset,
                });
            }
            if !compact_paragraphs {
                all_lines.push(RenderedLine {
                    text: String::new(),
                    chapter_idx: ch_idx,
                    para_idx: p_idx,
                    char_offset: 0,
                });
            }
        }
    }

    if all_lines.is_empty() {
        all_lines.push(RenderedLine {
            text: "(Empty book)".to_string(),
            chapter_idx: 0,
            para_idx: 0,
            char_offset: 0,
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
            start_char: first.char_offset,
        });
        idx = end;
    }

    if pages.is_empty() {
        pages.push(Page {
            lines: vec!["(Empty book)".to_string()],
            start_chapter: 0,
            start_para: 0,
            start_char: 0,
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

pub fn find_page_for_position(
    pages: &[Page],
    chapter: usize,
    para: usize,
    char_offset: usize,
) -> usize {
    if pages.is_empty() {
        return 0;
    }
    let target = (chapter, para, char_offset);
    let idx = pages.partition_point(|p| (p.start_chapter, p.start_para, p.start_char) <= target);
    idx.saturating_sub(1)
}

fn wrap_paragraph(para: &str, width: usize, indent: bool) -> Vec<(String, usize)> {
    let mut lines = Vec::new();
    let mut current = String::new();
    let mut current_len = 0usize;
    let mut line_start_char = 0usize;
    let mut consumed_chars = 0usize;

    for raw_word in para.split_whitespace() {
        if current.is_empty() && lines.is_empty() && indent && width > 4 {
            current.push_str("  ");
            current_len = 2;
        }
        // Pala One-style breakable punctuation splitting (hyphens, slashes, etc.).
        let pieces = split_after_breakable(raw_word);
        for (piece_idx, word) in pieces.iter().enumerate() {
            let need_space = piece_idx == 0 && !current.is_empty() && current != "  ";
            let word_len = UnicodeWidthStr::width(*word);
            let word_chars = word.chars().count();
            let gap_chars = usize::from(piece_idx == 0 && consumed_chars > 0);
            let piece_start_char = consumed_chars + gap_chars;
            let extra = usize::from(need_space);

            if current.is_empty() {
                line_start_char = piece_start_char;
                if word_len <= width {
                    current.push_str(word);
                    current_len = word_len;
                } else {
                    push_hard_wrapped(
                        word,
                        width,
                        piece_start_char,
                        &mut lines,
                        &mut current,
                        &mut current_len,
                        &mut line_start_char,
                    );
                }
            } else if current_len + extra + word_len <= width {
                if need_space {
                    current.push(' ');
                }
                current.push_str(word);
                current_len += extra + word_len;
            } else {
                lines.push((std::mem::take(&mut current), line_start_char));
                current_len = 0;
                line_start_char = piece_start_char;
                if word_len <= width {
                    current.push_str(word);
                    current_len = word_len;
                } else {
                    push_hard_wrapped(
                        word,
                        width,
                        piece_start_char,
                        &mut lines,
                        &mut current,
                        &mut current_len,
                        &mut line_start_char,
                    );
                }
            }
            consumed_chars = piece_start_char + word_chars;
        }
    }

    if !current.is_empty() {
        lines.push((current, line_start_char));
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
    word_start_char: usize,
    lines: &mut Vec<(String, usize)>,
    current: &mut String,
    current_len: &mut usize,
    line_start_char: &mut usize,
) {
    let mut chunk = String::new();
    let mut len = 0;
    let mut chunk_start = word_start_char;
    let mut offset = word_start_char;
    for ch in word.chars() {
        let cw = ch.width().unwrap_or(0);
        if len > 0 && len + cw > width {
            lines.push((std::mem::take(&mut chunk), chunk_start));
            len = 0;
            chunk_start = offset;
        }
        chunk.push(ch);
        len += cw;
        offset += 1;
        if len >= width {
            lines.push((std::mem::take(&mut chunk), chunk_start));
            len = 0;
            chunk_start = offset;
        }
    }
    if !chunk.is_empty() {
        *current = chunk;
        *current_len = len;
        *line_start_char = chunk_start;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::book::Chapter;
    use std::path::PathBuf;

    fn wrapped_lines(para: &str, width: usize) -> Vec<String> {
        wrap_paragraph(para, width, false)
            .into_iter()
            .map(|(line, _)| line)
            .collect()
    }

    #[test]
    fn hyphen_and_slash_wrapping_and_hard_wrap() {
        let wrapped = wrapped_lines("twenty-seven-year-old supercalifragilistic end-", 14);
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
            wrapped_lines("https://cardputer.cc/docs", 16),
            vec!["https://", "cardputer.cc/", "docs"]
        );
        // Exact-width hard-wrapped word (no remainder)
        assert_eq!(wrapped_lines("abcd", 2), vec!["ab", "cd"]);
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
        let c2_page = find_page_for_position(&relaxed.pages, 1, 0, 0);
        assert_eq!(relaxed.pages.get(c2_page).map(|p| p.start_chapter), Some(1));
    }

    #[test]
    fn sub_paragraph_position_lookup_resumes_exact_page() {
        let long_para = (0..40)
            .map(|i| format!("word{i:02}"))
            .collect::<Vec<_>>()
            .join(" ");
        let book = Book {
            title: "Long".to_string(),
            path: PathBuf::from("long.txt"),
            chapters: vec![Chapter {
                title: "C1".to_string(),
                paragraphs: vec![long_para],
            }],
        };
        let pag = paginate_book(&book, 20, 4, false);
        assert!(pag.pages.len() >= 4);
        for (idx, page) in pag.pages.iter().enumerate() {
            let found = find_page_for_position(
                &pag.pages,
                page.start_chapter,
                page.start_para,
                page.start_char,
            );
            assert_eq!(found, idx);
        }
    }
}
