# zread

A lightweight, distraction-free TUI e-reader written in Rust (`ratatui` + `crossterm`) for the **M5Stack Cardputer Zero** (`aarch64` Linux, 320×170 display) and other small-screen handhelds.

## Features

* **Formats:** EPUB (EPUB 2 `toc.ncx` & EPUB 3 `nav.xhtml`), Markdown (`.md`), and plain text (`.txt`).
* **Small-Screen Ergonomics:**
  * Hyphen-aware word wrapping and smart punctuation normalization (curly quotes, em-dashes, ellipses, non-breaking spaces) inspired by `pala-one-rs`.
  * OCR EPUB cleanup (strips `pagenum` / `pagebreak` spans and boilerplate) and automatic sectioning for TOC-less scanned EPUBs.
  * Configurable margins (`M`), compact paragraph spacing (`P`), and 4 low-glare color themes (`Warm Dark`, `Amber`, `Paper`, `Terminal`).
* **Cardputer Keypad First:** Diamond navigation (`f` Up, `x` Down, `z` Left, `c` Right) plus standard arrow keys and `Space`.
* **Built-in Tools:**
  * **Table of Contents (`t`)** with page numbers and `[` / `]` chapter jumps.
  * **Bookmarks (`m` / `b`)** persisted per book.
  * **Quick Notes (`N`)** appended as Markdown to `~/Books/Notes/<Book>.md`.
  * **Full-Text Search (`/`)** across the current book.
  * **RSVP Speed-Reading (`r`)** with ORP focus highlighting and adjustable WPM (`80`–`800`).
  * **Online Book Search (`g` in Library):** Search and download free public-domain EPUBs from **Standard Ebooks** (`[SE]`), **Project Gutenberg** (`[PG]` via Gutendex), and **Internet Archive** (`[IA]`).
* **Auto-Resume & Safe Exit:** Resumes your last book and paragraph on launch; short-pressing `Esc` backs out of any menu, while holding `Esc` for 3 seconds saves and quits.

---

## Keybindings

### Reader
| Key | Action |
| :--- | :--- |
| `c` / `x` / `Space` / `→` / `↓` | Next page |
| `z` / `f` / `←` / `↑` | Previous page |
| `t` | Table of Contents |
| `[` / `]` | Previous / Next chapter |
| `g` / `G` | First / Last page |
| `m` / `b` | Add bookmark / Manage bookmarks |
| `N` | Add quick note (`~/Books/Notes/<Book>.md`) |
| `/` | Search in book |
| `r` | Toggle RSVP speed-reading |
| `v` | Cycle color theme |
| `M` | Cycle margins (`1`, `2`, `4` chars) |
| `P` | Toggle compact paragraph spacing |
| `l` / `Esc` | Open Library |
| `?` | Scrollable command help |
| Hold `Esc` (3s) | Save progress & quit |

### Library & Online Search
| Key | Action |
| :--- | :--- |
| `f` / `x` / `↑` / `↓` | Move selection |
| `c` / `Enter` / `Space` | Open / Download selected book |
| `g` | Search online books (`SE` / `PG` / `IA`) |
| `d` / `Backspace` | Delete book (with confirmation; built-in guides protected) |
| `R` | Refresh library |
| `v` | Cycle color theme |

---

## Configuration & Data

* **Config:** `~/.config/zread/config.json` (auto-created on first run):
  ```json
  {
    "library_dir": "~/Books",
    "extra_dirs": ["~/Documents"],
    "theme": "Warm Dark",
    "font_size": 10,
    "margin": 1,
    "compact_paragraphs": false,
    "wpm": 260,
    "resume_last_book": true,
    "esc_quit_hold_ms": 3000
  }
  ```
* **Reading Progress & Bookmarks:** `~/.local/share/zread/state.json`
* **Quick Notes:** `~/Books/Notes/<Book>.md`

---

## Building & Deploying to Cardputer Zero

Requires Rust with the `aarch64-unknown-linux-musl` target and `rust-lld` (configured in [.cargo/config.toml](.cargo/config.toml)):

```bash
rustup target add aarch64-unknown-linux-musl
cargo build --release --target aarch64-unknown-linux-musl
```

### Deploy binary & launcher entry

1. Copy the static binary to `/usr/local/bin/zread` (or run `./deploy.sh pi@<device-ip>`).
2. Install the launcher icon and `.desktop` entry:
   ```bash
   sudo install -m 644 zread_icon.png /usr/share/APPLaunch/share/images/zread.png
   sudo install -m 644 zread.desktop /usr/share/APPLaunch/applications/zread.desktop
   ```
3. Restart `APPLaunch.service` (or reboot) to pick up the new launcher icon.

---

## Development & Testing

`Cargo.toml` enforces `unsafe_code = "forbid"`, `clippy::all = "deny"`, `clippy::pedantic = "deny"`, `unwrap_used = "deny"`, `expect_used = "deny"`, `panic = "deny"`, and `indexing_slicing = "deny"`.

```bash
cargo clippy --all-targets
cargo test
cargo mutants
```

## License

MIT — see [LICENSE](LICENSE).
