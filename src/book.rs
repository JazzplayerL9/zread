use std::collections::HashMap;
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};
use zip::ZipArchive;

#[derive(Debug, Clone)]
pub struct Chapter {
    pub title: String,
    pub paragraphs: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct Book {
    #[allow(dead_code)]
    pub title: String,
    pub path: PathBuf,
    pub chapters: Vec<Chapter>,
}

impl Book {
    pub fn load(path: &Path) -> Result<Self, String> {
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .map_or_default(str::to_ascii_lowercase);

        match ext.as_str() {
            "epub" => load_epub(path),
            "md" => load_markdown(path),
            _ => load_text(path),
        }
    }
}

fn fallback_title(path: &Path) -> String {
    path.file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("Untitled")
        .to_string()
}

fn load_text(path: &Path) -> Result<Book, String> {
    let content = std::fs::read_to_string(path).map_err(|e| format!("Failed to read file: {e}"))?;
    let paragraphs = split_plain_paragraphs(&content);
    Ok(Book {
        title: fallback_title(path),
        path: path.to_path_buf(),
        chapters: vec![Chapter {
            title: fallback_title(path),
            paragraphs,
        }],
    })
}

fn load_markdown(path: &Path) -> Result<Book, String> {
    let content = std::fs::read_to_string(path).map_err(|e| format!("Failed to read file: {e}"))?;
    let mut chapters: Vec<Chapter> = Vec::new();
    let mut cur_title = fallback_title(path);
    let mut cur_buf = String::new();

    for line in content.lines() {
        if let Some(heading) = line.strip_prefix("# ") {
            let paras = split_plain_paragraphs(&cur_buf);
            if !paras.is_empty() {
                chapters.push(Chapter {
                    title: cur_title.clone(),
                    paragraphs: paras,
                });
            }
            cur_title = heading.trim().to_string();
            cur_buf.clear();
            cur_buf.push_str(heading.trim());
            cur_buf.push_str("\n\n");
        } else {
            cur_buf.push_str(line);
            cur_buf.push('\n');
        }
    }

    let paras = split_plain_paragraphs(&cur_buf);
    if !paras.is_empty() || chapters.is_empty() {
        chapters.push(Chapter {
            title: cur_title,
            paragraphs: paras,
        });
    }

    Ok(Book {
        title: fallback_title(path),
        path: path.to_path_buf(),
        chapters,
    })
}

fn split_plain_paragraphs(text: &str) -> Vec<String> {
    let mut paragraphs = Vec::new();
    let mut current = String::new();

    for raw_line in text.trim_start_matches('\u{feff}').lines() {
        let trimmed = raw_line.trim();
        if trimmed.is_empty() {
            if !current.is_empty() {
                paragraphs.push(std::mem::take(&mut current));
            }
        } else {
            if !current.is_empty() {
                current.push(' ');
            }
            current.push_str(trimmed);
        }
    }
    if !current.is_empty() {
        paragraphs.push(current);
    }
    paragraphs
}

fn load_epub(path: &Path) -> Result<Book, String> {
    let file = File::open(path).map_err(|e| format!("Failed to open EPUB: {e}"))?;
    let mut archive = ZipArchive::new(file).map_err(|e| format!("Invalid EPUB zip: {e}"))?;

    let container_xml = read_zip_string(&mut archive, "META-INF/container.xml")?;
    let opf_path = parse_container_opf(&container_xml)?;
    let opf_dir = opf_path
        .rsplit_once('/')
        .map_or_default(|(dir, _)| format!("{dir}/"));

    let opf_xml = read_zip_string(&mut archive, &opf_path)?;
    let opf_doc =
        roxmltree::Document::parse(&opf_xml).map_err(|e| format!("Invalid OPF XML: {e}"))?;

    let book_title = opf_doc
        .descendants()
        .find(|n| n.tag_name().name() == "title")
        .and_then(|n| n.text())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| fallback_title(path));

    let mut manifest: HashMap<String, String> = HashMap::new();
    let mut ncx_href: Option<String> = None;
    let mut nav_href: Option<String> = None;

    for item in opf_doc
        .descendants()
        .filter(|n| n.tag_name().name() == "item")
    {
        if let (Some(id), Some(href)) = (item.attribute("id"), item.attribute("href")) {
            let full_href = resolve_rel(&opf_dir, href);
            if item.attribute("media-type") == Some("application/x-dtbncx+xml") {
                ncx_href = Some(full_href.clone());
            }
            if item
                .attribute("properties")
                .is_some_and(|p| p.split_whitespace().any(|w| w == "nav"))
            {
                nav_href = Some(full_href.clone());
            }
            manifest.insert(id.to_string(), full_href);
        }
    }

    let mut toc_titles = ncx_href
        .as_deref()
        .and_then(|href| {
            let dir = href.rsplit_once('/').map_or("", |(d, _)| d);
            let base = if dir.is_empty() { String::new() } else { format!("{dir}/") };
            read_zip_string(&mut archive, href)
                .ok()
                .map(|xml| parse_ncx_titles(&xml, &base))
        })
        .unwrap_or_default();

    if let Some(href) = nav_href.as_deref() {
        let dir = href.rsplit_once('/').map_or("", |(d, _)| d);
        let base = if dir.is_empty() { String::new() } else { format!("{dir}/") };
        if let Ok(xml) = read_zip_string(&mut archive, href) {
            for (k, v) in parse_nav_titles(&xml, &base) {
                toc_titles.entry(k).or_insert(v);
            }
        }
    }

    let spine_ids: Vec<String> = opf_doc
        .descendants()
        .filter(|n| n.tag_name().name() == "itemref")
        .filter_map(|n| n.attribute("idref").map(str::to_string))
        .collect();

    let mut chapters: Vec<Chapter> = Vec::new();
    for (idx, idref) in spine_ids.iter().enumerate() {
        let Some(href) = manifest.get(idref) else {
            continue;
        };
        let Ok(xhtml) = read_zip_string(&mut archive, href) else {
            continue;
        };
        let (detected_title, paragraphs) = extract_xhtml_content(&xhtml);
        if paragraphs.is_empty() {
            continue;
        }
        let toc_title = toc_titles.get(href).cloned();
        let starts_new_chapter =
            if chapters.is_empty() || toc_title.is_some() || detected_title.is_some() {
                true
            } else if toc_titles.is_empty() && spine_ids.len() > 30 {
                // Scanned/OCR EPUB with no TOC: split every ~25 spine pages so TOC stays navigable.
                idx % 25 == 0
            } else {
                false
            };

        if starts_new_chapter {
            let fallback_heading = paragraphs.iter().take(3).find_map(|p| {
                let len = p.chars().count();
                let alpha = p.chars().filter(char::is_ascii_alphabetic).count();
                if (4..=45).contains(&len)
                    && alpha * 2 >= len
                    && !p.ends_with([',', ';', '.'])
                {
                    Some(p.clone())
                } else {
                    None
                }
            });
            let title = toc_title
                .or(detected_title)
                .or(fallback_heading)
                .unwrap_or_else(|| format!("Part {}", chapters.len() + 1));
            chapters.push(Chapter { title, paragraphs });
        } else if let Some(cur) = chapters.last_mut() {
            merge_continuation_paragraphs(&mut cur.paragraphs, paragraphs);
        }
    }

    if chapters.is_empty() {
        return Err("No readable chapters found in EPUB".to_string());
    }

    Ok(Book {
        title: book_title,
        path: path.to_path_buf(),
        chapters,
    })
}

fn merge_continuation_paragraphs(existing: &mut Vec<String>, incoming: Vec<String>) {
    let mut iter = incoming.into_iter();
    if let (Some(prev), Some(first)) = (existing.last_mut(), iter.next()) {
        let ends_sentence = prev.ends_with(['.', '!', '?', ':', '"', '\u{201d}', '\'', '\u{2019}', ')']);
        let starts_lower = first.chars().next().is_some_and(char::is_lowercase);
        if prev.ends_with('-') && prev.len() > 1 {
            prev.pop();
            prev.push_str(&first);
        } else if !ends_sentence && starts_lower {
            prev.push(' ');
            prev.push_str(&first);
        } else {
            existing.push(first);
        }
    }
    existing.extend(iter);
}

fn read_zip_string(archive: &mut ZipArchive<File>, name: &str) -> Result<String, String> {
    let mut entry = archive
        .by_name(name)
        .map_err(|e| format!("Missing '{name}' in EPUB: {e}"))?;
    let mut bytes = Vec::new();
    entry
        .read_to_end(&mut bytes)
        .map_err(|e| format!("Failed reading '{name}': {e}"))?;
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

fn parse_container_opf(xml: &str) -> Result<String, String> {
    let doc = roxmltree::Document::parse(xml).map_err(|e| format!("Invalid container.xml: {e}"))?;
    doc.descendants()
        .find(|n| n.tag_name().name() == "rootfile")
        .and_then(|n| n.attribute("full-path"))
        .map(str::to_string)
        .ok_or_else(|| "Missing rootfile full-path in container.xml".to_string())
}

fn parse_ncx_titles(xml: &str, opf_dir: &str) -> HashMap<String, String> {
    let mut titles = HashMap::new();
    let Ok(doc) = roxmltree::Document::parse(xml) else {
        return titles;
    };
    for nav in doc
        .descendants()
        .filter(|n| n.tag_name().name() == "navPoint")
    {
        let label = nav
            .descendants()
            .find(|n| n.tag_name().name() == "text")
            .and_then(|n| n.text())
            .map_or("", str::trim);
        let src = nav
            .descendants()
            .find(|n| n.tag_name().name() == "content")
            .and_then(|n| n.attribute("src"))
            .unwrap_or("");
        if !label.is_empty() && !src.is_empty() {
            let clean_src = src.split('#').next().unwrap_or(src);
            titles
                .entry(resolve_rel(opf_dir, clean_src))
                .or_insert_with(|| label.to_string());
        }
    }
    titles
}

fn resolve_rel(base_dir: &str, href: &str) -> String {
    let decoded = percent_decode(href);
    let combined = format!("{base_dir}{decoded}");
    let mut parts: Vec<&str> = Vec::new();
    for seg in combined.split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            s => parts.push(s),
        }
    }
    parts.join("/")
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while let Some(&b) = bytes.get(i) {
        if b == b'%' {
            if let Some(hex_bytes) = bytes.get(i + 1..i + 3) {
                if let Ok(hex) = std::str::from_utf8(hex_bytes) {
                    if let Ok(val) = u8::from_str_radix(hex, 16) {
                        out.push(val);
                        i += 3;
                        continue;
                    }
                }
            }
        }
        out.push(b);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn parse_nav_titles(xml: &str, nav_dir: &str) -> HashMap<String, String> {
    let mut titles = HashMap::new();
    let Ok(doc) = roxmltree::Document::parse(xml) else {
        return titles;
    };
    for a in doc
        .descendants()
        .filter(|n| n.is_element() && n.tag_name().name().eq_ignore_ascii_case("a"))
    {
        if let Some(href) = a.attribute("href") {
            let label = normalize_ws(&collect_inline_text(a));
            if !label.is_empty() {
                let clean_src = href.split('#').next().unwrap_or(href);
                if !clean_src.is_empty() {
                    titles
                        .entry(resolve_rel(nav_dir, clean_src))
                        .or_insert(label);
                }
            }
        }
    }
    titles
}

fn is_page_artifact(s: &str) -> bool {
    let t = s.trim();
    if t.is_empty() {
        return true;
    }
    let lower = t.to_ascii_lowercase();
    if lower == "digitized by microsoft®"
        || lower == "digitized by google"
        || lower.starts_with("the text on this page is estimated to be only")
    {
        return true;
    }
    if t.len() > 18 {
        return false;
    }
    if t.chars().all(|c| c.is_ascii_digit()) {
        return true;
    }
    if let Some(rest) = lower
        .strip_prefix("page ")
        .or_else(|| lower.strip_prefix("p. "))
        .or_else(|| lower.strip_prefix("pg. "))
    {
        return rest
            .trim()
            .trim_matches(|c: char| matches!(c, '[' | ']' | '(' | ')' | '.'))
            .chars()
            .all(|c| c.is_ascii_digit() || c == 'i' || c == 'v' || c == 'x' || c == 'l');
    }
    false
}

fn extract_xhtml_content(xhtml: &str) -> (Option<String>, Vec<String>) {
    if let Ok(doc) = roxmltree::Document::parse(xhtml) {
        let mut title = None;
        let mut paragraphs = Vec::new();
        let body = doc
            .descendants()
            .find(|n| n.tag_name().name().eq_ignore_ascii_case("body"))
            .unwrap_or_else(|| doc.root());

        for node in body.descendants() {
            if !node.is_element() {
                continue;
            }
            let tag = node.tag_name().name().to_ascii_lowercase();
            if matches!(
                tag.as_str(),
                "h1" | "h2" | "h3" | "h4" | "p" | "li" | "blockquote"
            ) {
                let text = collect_inline_text(node);
                let normalized = normalize_ws(&text);
                if !normalized.is_empty() && !is_page_artifact(&normalized) {
                    if title.is_none() && matches!(tag.as_str(), "h1" | "h2" | "h3" | "h4") {
                        title = Some(normalized.clone());
                    }
                    if tag == "li" {
                        paragraphs.push(format!("• {normalized}"));
                    } else {
                        paragraphs.push(normalized);
                    }
                }
            }
        }

        if !paragraphs.is_empty() {
            return (title, paragraphs);
        }
    }

    // Fallback tag-stripper for non-well-formed HTML.
    let stripped = strip_html_tags(xhtml);
    let paragraphs = split_plain_paragraphs(&stripped)
        .into_iter()
        .filter(|p| !is_page_artifact(p))
        .collect();
    (None, paragraphs)
}

fn is_pagebreak_node(node: roxmltree::Node) -> bool {
    if !node.is_element() {
        return false;
    }
    let has_pb_class = node
        .attribute("class")
        .is_some_and(|c| c.to_ascii_lowercase().contains("pagenum") || c.to_ascii_lowercase().contains("pagebreak"));
    let has_pb_type = node
        .attributes()
        .any(|a| a.name().ends_with("type") && a.value().eq_ignore_ascii_case("pagebreak"));
    has_pb_class || has_pb_type
}

fn collect_inline_text(node: roxmltree::Node) -> String {
    let mut out = String::new();
    for child in node.descendants() {
        if child.ancestors().any(is_pagebreak_node) {
            continue;
        }
        if child.is_text() {
            if let Some(t) = child.text() {
                out.push_str(t);
            }
        } else if child.is_element() && child.tag_name().name().eq_ignore_ascii_case("br") {
            out.push(' ');
        }
    }
    out
}

fn normalize_ws(s: &str) -> String {
    let mut normalized_punct = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '\u{feff}' => {}
            '\u{2018}' | '\u{2019}' | '\u{201a}' | '`' => normalized_punct.push('\''),
            '\u{201c}' | '\u{201d}' | '\u{201e}' | '\u{00ab}' | '\u{00bb}' | '\u{2039}'
            | '\u{203a}' => normalized_punct.push('"'),
            '\u{2013}' | '\u{2014}' | '\u{2015}' => normalized_punct.push('-'),
            '\u{2026}' => normalized_punct.push_str("..."),
            '\u{00a0}' | '\r' => normalized_punct.push(' '),
            other => normalized_punct.push(other),
        }
    }
    normalized_punct
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn strip_html_tags(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut in_tag = false;
    let mut skip_block = false;
    let lower = input.to_ascii_lowercase();
    let bytes = input.as_bytes();
    let mut i = 0;

    while let Some(&b) = bytes.get(i) {
        if !in_tag && b == b'<' {
            let rest = lower.get(i..).unwrap_or("");
            if rest.starts_with("<style") || rest.starts_with("<script") {
                skip_block = true;
            } else if rest.starts_with("</style") || rest.starts_with("</script") {
                skip_block = false;
            } else if rest.starts_with("<p")
                || rest.starts_with("<div")
                || rest.starts_with("<br")
                || rest.starts_with("<h")
            {
                out.push('\n');
            }
            in_tag = true;
            i += 1;
            continue;
        }
        if in_tag {
            if b == b'>' {
                in_tag = false;
            }
            i += 1;
            continue;
        }
        if !skip_block {
            if let Some(ch) = input.get(i..).and_then(|s| s.chars().next()) {
                out.push(ch);
                i += ch.len_utf8();
                continue;
            }
        }
        i += 1;
    }

    decode_entities(&out)
}

fn decode_entities(s: &str) -> String {
    s.replace("&nbsp;", " ")
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&#39;", "'")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use zip::write::SimpleFileOptions;

    #[test]
    fn punctuation_and_artifact_filtering() {
        assert_eq!(
            normalize_ws("\u{feff}“Hello”―world…\u{00a0}‘test’‚„‹›"),
            "\"Hello\"-world... 'test''\"\"\""
        );
        assert!(is_page_artifact(""));
        assert!(is_page_artifact("42"));
        assert!(is_page_artifact("Page 12"));
        assert!(is_page_artifact("pg. [xiv]"));
        assert!(is_page_artifact("p. [iv]"));
        assert!(is_page_artifact("Digitized by Microsoft®"));
        assert!(!is_page_artifact("1234567890123456789")); // > 18 chars
        assert!(!is_page_artifact("This is a normal sentence."));
        assert_eq!(fallback_title(Path::new("/tmp/MyBook.txt")), "MyBook");
        assert_eq!(
            split_plain_paragraphs("Line one\nline two\n\nPara two"),
            vec!["Line one line two", "Para two"]
        );
    }

    #[test]
    fn continuation_paragraph_reflow_and_hyphen_join() {
        let mut existing = vec!["The quick brown fox jump-".to_string()];
        merge_continuation_paragraphs(
            &mut existing,
            vec!["ed over".to_string(), "Next para.".to_string()],
        );
        assert_eq!(existing, vec!["The quick brown fox jumped over", "Next para."]);

        let mut mid_sentence = vec!["It was the best of".to_string()];
        merge_continuation_paragraphs(&mut mid_sentence, vec!["times.".to_string()]);
        assert_eq!(mid_sentence, vec!["It was the best of times."]);

        // Single-char '-' should not strip as hyphenated word
        let mut dash = vec!["-".to_string()];
        merge_continuation_paragraphs(&mut dash, vec!["next".to_string()]);
        assert_eq!(dash, vec!["- next"]);
    }

    #[test]
    fn nav_xhtml_and_html_fallback_and_chunking() {
        let nav_xml = r#"<html xmlns="http://www.w3.org/1999/xhtml"><body><nav><ol><li><a href="ch1.xhtml#p1">Part One</a></li></ol></nav></body></html>"#;
        let nav = parse_nav_titles(nav_xml, "OEBPS/");
        assert_eq!(nav.get("OEBPS/ch1.xhtml").map(String::as_str), Some("Part One"));

        let html = "<p>A</p><div>B</div><br/><h1>C</h1>";
        let stripped = strip_html_tags(html);
        assert!(stripped.contains("A\nB\n\nC"));

        let (_t, paras) = extract_xhtml_content("<div><p>One</p><p>Two</p></div>");
        assert_eq!(paras, vec!["One", "Two"]);
    }

    #[test]
    fn markdown_chapter_splitting_and_epub_loading() {
        let md_path = std::env::temp_dir().join("zread_test_md.md");
        let _ = std::fs::write(&md_path, "# First\n\nHello world.\n\n# Second\n\nBye world.\n");
        let md_book = Book::load(&md_path);
        let _ = std::fs::remove_file(&md_path);
        assert!(md_book.is_ok());
        if let Ok(b) = md_book {
            assert_eq!(b.chapters.len(), 2);
            assert_eq!(b.chapters.first().map(|c| c.title.as_str()), Some("First"));
            assert_eq!(b.chapters.get(1).map(|c| c.title.as_str()), Some("Second"));
        }

        // Build a minimal real EPUB in temp_dir and load it
        let epub_path = std::env::temp_dir().join("zread_test_real.epub");
        if let Ok(file) = File::create(&epub_path) {
            let mut z = zip::ZipWriter::new(file);
            let opts = SimpleFileOptions::default();
            let _ = z.start_file("META-INF/container.xml", opts);
            let _ = z.write_all(
                br#"<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container"><rootfiles><rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/></rootfiles></container>"#,
            );
            let _ = z.start_file("OEBPS/content.opf", opts);
            let _ = z.write_all(
                br#"<package xmlns="http://www.idpf.org/2007/opf" version="2.0"><metadata xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:title>Sample</dc:title></metadata><manifest><item id="ncx" href="toc.ncx" media-type="application/x-dtbncx+xml"/><item id="c1" href="ch1.xhtml" media-type="application/xhtml+xml"/></manifest><spine toc="ncx"><itemref idref="c1"/></spine></package>"#,
            );
            let _ = z.start_file("OEBPS/toc.ncx", opts);
            let _ = z.write_all(
                br#"<ncx xmlns="http://www.daisy.org/z3986/2005/ncx/"><navMap><navPoint id="n1"><navLabel><text>Intro</text></navLabel><content src="ch1.xhtml"/></navPoint></navMap></ncx>"#,
            );
            let _ = z.start_file("OEBPS/ch1.xhtml", opts);
            let _ = z.write_all(
                br#"<html xmlns="http://www.w3.org/1999/xhtml"><body><p>Hello <span class="pagenum">99</span>EPUB.</p></body></html>"#,
            );
            let _ = z.finish();
        }
        let epub = Book::load(&epub_path);
        let _ = std::fs::remove_file(&epub_path);
        assert!(epub.is_ok());
        if let Ok(b) = epub {
            assert_eq!(b.title, "Sample");
            assert_eq!(b.chapters.first().map(|c| c.title.as_str()), Some("Intro"));
            assert_eq!(
                b.chapters.first().and_then(|c| c.paragraphs.first()).map(String::as_str),
                Some("Hello EPUB.")
            );
        }
    }

    #[test]
    fn resolve_rel_and_percent_decode() {
        assert_eq!(resolve_rel("OEBPS/", "../Text/ch%201.xhtml"), "Text/ch 1.xhtml");
        assert_eq!(strip_html_tags("<p>Hi<style>x</style> &amp; Bye</p>").trim(), "Hi & Bye");
    }
}
