use serde::Deserialize;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Clone)]
pub struct CatalogBook {
    pub id: String,
    pub source: &'static str,
    pub title: String,
    pub author: String,
    pub epub_url: Option<String>,
    pub txt_url: Option<String>,
    pub ia_identifier: Option<String>,
}

#[derive(Deserialize)]
struct GutendexResponse {
    results: Vec<GutendexItem>,
}

#[derive(Deserialize)]
struct GutendexItem {
    id: u64,
    title: String,
    authors: Vec<GutendexAuthor>,
    formats: std::collections::HashMap<String, String>,
}

#[derive(Deserialize)]
struct GutendexAuthor {
    name: String,
}

#[derive(Deserialize)]
struct IaResponse {
    response: IaDocs,
}

#[derive(Deserialize)]
struct IaDocs {
    docs: Vec<IaDoc>,
}

#[derive(Deserialize)]
struct IaDoc {
    identifier: String,
    title: Option<serde_json::Value>,
    creator: Option<serde_json::Value>,
}

#[derive(Deserialize)]
struct IaFilesResponse {
    result: Vec<IaFile>,
}

#[derive(Deserialize)]
struct IaFile {
    name: String,
    private: Option<String>,
}

pub fn search_catalog(query: &str) -> Result<Vec<CatalogBook>, String> {
    let trimmed = query.trim();
    if trimmed.is_empty() {
        return Ok(Vec::new());
    }

    let mut combined = Vec::new();
    if let Ok(se_books) = search_standard_ebooks(trimmed) {
        combined.extend(se_books);
    }
    if let Ok(pg_books) = search_gutendex(trimmed) {
        combined.extend(pg_books);
    }
    if !combined.is_empty() {
        combined.truncate(25);
        return Ok(combined);
    }

    search_archive_org(trimmed)
}

fn search_standard_ebooks(query: &str) -> Result<Vec<CatalogBook>, String> {
    let encoded = url_encode(query);
    let url = format!("https://standardebooks.org/ebooks?query={encoded}");
    let output = Command::new("curl")
        .args(["-fsSL", "--max-time", "8", "-A", "zread/0.2", &url])
        .output()
        .map_err(|e| format!("Failed to run curl: {e}"))?;

    if !output.status.success() {
        return Err("Standard Ebooks unavailable".to_string());
    }

    let html = String::from_utf8_lossy(&output.stdout);
    Ok(parse_standard_ebooks_html(&html))
}

fn parse_standard_ebooks_html(html: &str) -> Vec<CatalogBook> {
    let mut books = Vec::new();
    for chunk in html.split("<li typeof=\"schema:Book\" about=\"").skip(1) {
        let Some(slug_path) = chunk.split('"').next() else {
            continue;
        };
        let Some(slug) = slug_path.strip_prefix("/ebooks/") else {
            continue;
        };
        if slug.is_empty() {
            continue;
        }
        let end_li = chunk.find("</li>").unwrap_or(chunk.len());
        let item = chunk.get(..end_li).unwrap_or(chunk);

        let mut names = extract_schema_names(item);
        if names.is_empty() {
            continue;
        }
        let title = names.remove(0);
        let author = if names.is_empty() {
            "Unknown".to_string()
        } else {
            names.join(", ")
        };
        let file_slug = slug.replace('/', "_");
        let epub_url = format!(
            "https://standardebooks.org/ebooks/{slug}/downloads/{file_slug}.epub?source=download"
        );
        books.push(CatalogBook {
            id: file_slug,
            source: "SE",
            title,
            author,
            epub_url: Some(epub_url),
            txt_url: None,
            ia_identifier: None,
        });
    }
    books
}

fn extract_schema_names(html: &str) -> Vec<String> {
    let marker = "<span property=\"schema:name\">";
    let mut names = Vec::new();
    for part in html.split(marker).skip(1) {
        if let Some((raw, _)) = part.split_once("</span>") {
            let decoded = decode_basic_entities(raw.trim());
            if !decoded.is_empty() {
                names.push(decoded);
            }
        }
    }
    names
}

fn decode_basic_entities(s: &str) -> String {
    s.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&apos;", "'")
}

fn search_gutendex(query: &str) -> Result<Vec<CatalogBook>, String> {
    let encoded = url_encode(query);
    let url = format!("https://gutendex.com/books?search={encoded}");
    let output = Command::new("curl")
        .args(["-fsSL", "--max-time", "4", &url])
        .output()
        .map_err(|e| format!("Failed to run curl: {e}"))?;

    if !output.status.success() {
        return Err("Gutendex unavailable".to_string());
    }

    let parsed: GutendexResponse =
        serde_json::from_slice(&output.stdout).map_err(|e| format!("Bad JSON: {e}"))?;

    let books = parsed
        .results
        .into_iter()
        .take(25)
        .map(|item| {
            let author = item
                .authors
                .first().map_or_else(|| "Unknown".to_string(), |a| a.name.clone());
            let epub_url = item
                .formats
                .get("application/epub+zip")
                .cloned()
                .or_else(|| {
                    item.formats
                        .iter()
                        .find(|(k, _)| k.starts_with("application/epub+zip"))
                        .map(|(_, v)| v.clone())
                });
            let txt_url = item
                .formats
                .iter()
                .find(|(k, _)| k.starts_with("text/plain"))
                .map(|(_, v)| v.clone());

            CatalogBook {
                id: item.id.to_string(),
                source: "PG",
                title: item.title,
                author,
                epub_url,
                txt_url,
                ia_identifier: None,
            }
        })
        .filter(|b| b.epub_url.is_some() || b.txt_url.is_some())
        .collect();

    Ok(books)
}

fn search_archive_org(query: &str) -> Result<Vec<CatalogBook>, String> {
    let encoded = url_encode(&format!(
        "title:({query}) AND format:(EPUB) AND mediatype:(texts) AND NOT access-restricted-item:true"
    ));
    let url = format!(
        "https://archive.org/advancedsearch.php?q={encoded}&fl[]=identifier,title,creator&sort[]=downloads+desc&rows=20&output=json"
    );
    let output = Command::new("curl")
        .args(["-fsSL", "--max-time", "10", &url])
        .output()
        .map_err(|e| format!("Failed to run curl: {e}"))?;

    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr);
        return Err(format!("Search failed: {}", err.trim()));
    }

    let parsed: IaResponse =
        serde_json::from_slice(&output.stdout).map_err(|e| format!("Bad JSON: {e}"))?;

    let books = parsed
        .response
        .docs
        .into_iter()
        .map(|doc| {
            let author = match doc.creator {
                Some(serde_json::Value::String(s)) => s,
                Some(serde_json::Value::Array(arr)) => arr
                    .first()
                    .and_then(|v| v.as_str())
                    .unwrap_or("Unknown")
                    .to_string(),
                _ => "Unknown".to_string(),
            };
            let title = match doc.title {
                Some(serde_json::Value::String(s)) => s,
                Some(serde_json::Value::Array(arr)) => arr
                    .first()
                    .and_then(|v| v.as_str())
                    .unwrap_or(&doc.identifier)
                    .to_string(),
                _ => doc.identifier.clone(),
            };
            CatalogBook {
                id: doc.identifier.clone(),
                source: "IA",
                title,
                author,
                epub_url: None,
                txt_url: None,
                ia_identifier: Some(doc.identifier),
            }
        })
        .collect();

    Ok(books)
}

pub fn download_book(book: &CatalogBook, library_dir: &Path) -> Result<PathBuf, String> {
    let (url, ext) = if let Some(u) = &book.epub_url {
        (u.clone(), "epub")
    } else if let Some(u) = &book.txt_url {
        (u.clone(), "txt")
    } else if let Some(ident) = &book.ia_identifier {
        (resolve_ia_epub_url(ident)?, "epub")
    } else {
        return Err("No EPUB or TXT format available".to_string());
    };

    let _ = std::fs::create_dir_all(library_dir);
    let safe_name = sanitize_filename(&book.title);
    let dest = library_dir.join(format!("{safe_name}_{}.{ext}", sanitize_filename(&book.id)));

    let output = Command::new("curl")
        .args(["-fsSL", "--max-time", "45", "-A", "zread/0.2", "-o"])
        .arg(&dest)
        .arg(&url)
        .output()
        .map_err(|e| format!("Failed to run curl: {e}"))?;

    if !output.status.success() {
        let _ = std::fs::remove_file(&dest);
        let err = String::from_utf8_lossy(&output.stderr);
        return Err(format!("Download failed: {}", err.trim()));
    }

    Ok(dest)
}

fn resolve_ia_epub_url(identifier: &str) -> Result<String, String> {
    let meta_url = format!("https://archive.org/metadata/{identifier}/files");
    let output = Command::new("curl")
        .args(["-fsSL", "--max-time", "10", &meta_url])
        .output()
        .map_err(|e| format!("Failed to run curl: {e}"))?;

    if !output.status.success() {
        return Err("Metadata lookup failed".to_string());
    }

    let parsed: IaFilesResponse =
        serde_json::from_slice(&output.stdout).map_err(|e| format!("Bad metadata: {e}"))?;

    let epub_name = parsed
        .result
        .into_iter()
        .find(|f| {
            f.name.to_ascii_lowercase().ends_with(".epub")
                && f.private.as_deref() != Some("true")
        })
        .map(|f| f.name)
        .ok_or_else(|| "No public .epub file in item".to_string())?;

    let encoded_name = url_encode(&epub_name);
    Ok(format!("https://archive.org/download/{identifier}/{encoded_name}"))
}

fn sanitize_filename(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .take(40)
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    let trimmed = cleaned.trim_matches('_');
    if trimmed.is_empty() {
        "book".to_string()
    } else {
        trimmed.to_string()
    }
}

fn url_encode(s: &str) -> String {
    use std::fmt::Write;
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char);
            }
            b' ' => out.push_str("%20"),
            _ => {
                let _ = write!(out, "%{b:02X}");
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn url_encoding_and_sanitize() {
        assert_eq!(url_encode("walden & pond"), "walden%20%26%20pond");
        assert_eq!(sanitize_filename("Walden: Or, Life!"), "Walden__Or__Life");
        assert_eq!(sanitize_filename("///"), "book");
    }

    #[test]
    fn parse_standard_ebooks_search_html() {
        let sample = r#"
<ol>
<li typeof="schema:Book" about="/ebooks/edgar-allan-poe/short-fiction">
  <p><a href="/ebooks/edgar-allan-poe/short-fiction" property="schema:url"><span property="schema:name">Short Fiction &amp; Tales</span></a></p>
  <p class="author" typeof="schema:Person" property="schema:author" resource="/ebooks/edgar-allan-poe"><a href="https://standardebooks.org/ebooks/edgar-allan-poe" property="schema:url"><span property="schema:name">Edgar Allan Poe</span></a></p>
</li>
</ol>"#;
        let books = parse_standard_ebooks_html(sample);
        assert_eq!(books.len(), 1);
        assert_eq!(
            books.first().map(|b| (
                b.title.as_str(),
                b.author.as_str(),
                b.source,
                b.epub_url.as_deref(),
            )),
            Some((
                "Short Fiction & Tales",
                "Edgar Allan Poe",
                "SE",
                Some("https://standardebooks.org/ebooks/edgar-allan-poe/short-fiction/downloads/edgar-allan-poe_short-fiction.epub?source=download"),
            ))
        );
    }
}
