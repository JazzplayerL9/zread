use serde::Deserialize;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Clone)]
pub struct GutenbergBook {
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

pub fn search_gutenberg(query: &str) -> Result<Vec<GutenbergBook>, String> {
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

fn search_standard_ebooks(query: &str) -> Result<Vec<GutenbergBook>, String> {
    let encoded = url_encode(query);
    let url = format!("https://standardebooks.org/feeds/opds/all?query={encoded}");
    let output = Command::new("curl")
        .args(["-fsSL", "--max-time", "5", "-A", "zread/0.1", &url])
        .output()
        .map_err(|e| format!("Failed to run curl: {e}"))?;

    if !output.status.success() {
        return Err("Standard Ebooks unavailable".to_string());
    }

    let xml = String::from_utf8_lossy(&output.stdout);
    let doc = roxmltree::Document::parse(&xml).map_err(|e| format!("Bad OPDS XML: {e}"))?;

    let mut books = Vec::new();
    for entry in doc
        .descendants()
        .filter(|n| n.is_element() && n.tag_name().name() == "entry")
    {
        let title = entry
            .children()
            .find(|n| n.is_element() && n.tag_name().name() == "title")
            .and_then(|n| n.text())
            .map_or("", str::trim)
            .to_string();
        if title.is_empty() {
            continue;
        }
        let author = entry
            .children()
            .find(|n| n.is_element() && n.tag_name().name() == "author")
            .and_then(|a| {
                a.children()
                    .find(|n| n.is_element() && n.tag_name().name() == "name")
            })
            .and_then(|n| n.text())
            .map_or_else(|| "Unknown".to_string(), |s| s.trim().to_string());

        let epub_url = entry
            .children()
            .filter(|n| n.is_element() && n.tag_name().name() == "link")
            .find(|n| {
                n.attribute("type") == Some("application/epub+zip")
                    && n.attribute("title")
                        .is_some_and(|t| t.contains("Recommended"))
            })
            .or_else(|| {
                entry
                    .children()
                    .filter(|n| n.is_element() && n.tag_name().name() == "link")
                    .find(|n| n.attribute("type") == Some("application/epub+zip"))
            })
            .and_then(|n| n.attribute("href"))
            .map(str::to_string);

        if let Some(url) = epub_url {
            let id = entry
                .children()
                .find(|n| n.is_element() && n.tag_name().name() == "id")
                .and_then(|n| n.text())
                .and_then(|s| s.rsplit('/').next())
                .unwrap_or("se")
                .to_string();
            books.push(GutenbergBook {
                id,
                source: "SE",
                title,
                author,
                epub_url: Some(url),
                txt_url: None,
                ia_identifier: None,
            });
        }
    }

    Ok(books)
}

fn search_gutendex(query: &str) -> Result<Vec<GutenbergBook>, String> {
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

            GutenbergBook {
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

fn search_archive_org(query: &str) -> Result<Vec<GutenbergBook>, String> {
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
            GutenbergBook {
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

pub fn download_book(book: &GutenbergBook, library_dir: &Path) -> Result<PathBuf, String> {
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
        .args(["-fsSL", "--max-time", "45", "-o"])
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
}
