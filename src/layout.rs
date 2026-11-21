use crate::book::Book;

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
            for wrapped in wrap_paragraph(para, wrap_width) {
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

    for raw_word in para.split_whitespace() {
        // Pala One-style hyphen-aware splitting: allow breaking after '-' on narrow screens.
        let pieces = split_after_hyphens(raw_word);
        for (piece_idx, word) in pieces.iter().enumerate() {
            let need_space = piece_idx == 0 && !current.is_empty();
            let word_len = word.chars().count();
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

fn split_after_hyphens(word: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut start = 0;
    for (idx, ch) in word.char_indices() {
        if ch == '-' && idx + 1 < word.len() {
            if let Some(seg) = word.get(start..=idx) {
                parts.push(seg);
                start = idx + 1;
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
        chunk.push(ch);
        len += 1;
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
    fn hyphen_wrapping_and_hard_wrap() {
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
        // Relaxed has leading blank lines skipped at page boundaries
        assert!(relaxed.pages.iter().all(|p| p.lines.first().is_some_and(|l| !l.is_empty())));
        let c2_page = find_page_for_position(&relaxed.pages, 1, 0);
        assert_eq!(relaxed.pages.get(c2_page).map(|p| p.start_chapter), Some(1));
    }
}
