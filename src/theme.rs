use ratatui::style::Color;

#[derive(Debug, Clone, Copy)]
pub struct Theme {
    pub name: &'static str,
    pub bg: Color,
    pub fg: Color,
    pub accent: Color,
    pub muted: Color,
    pub status_bg: Color,
    pub status_fg: Color,
}

pub const THEMES: &[Theme] = &[
    Theme {
        name: "Warm Dark",
        bg: Color::Rgb(20, 18, 16),
        fg: Color::Rgb(214, 199, 169),
        accent: Color::Rgb(235, 180, 100),
        muted: Color::Rgb(120, 110, 95),
        status_bg: Color::Rgb(34, 30, 26),
        status_fg: Color::Rgb(175, 162, 138),
    },
    Theme {
        name: "Amber",
        bg: Color::Rgb(14, 10, 0),
        fg: Color::Rgb(255, 176, 0),
        accent: Color::Rgb(255, 220, 100),
        muted: Color::Rgb(140, 95, 0),
        status_bg: Color::Rgb(32, 22, 0),
        status_fg: Color::Rgb(215, 145, 0),
    },
    Theme {
        name: "Paper",
        bg: Color::Rgb(234, 228, 215),
        fg: Color::Rgb(34, 32, 30),
        accent: Color::Rgb(140, 60, 20),
        muted: Color::Rgb(125, 118, 106),
        status_bg: Color::Rgb(214, 206, 190),
        status_fg: Color::Rgb(55, 50, 45),
    },
    Theme {
        name: "Terminal",
        bg: Color::Reset,
        fg: Color::Reset,
        accent: Color::Cyan,
        muted: Color::DarkGray,
        status_bg: Color::DarkGray,
        status_fg: Color::White,
    },
];

pub fn theme_index_by_name(name: &str) -> usize {
    THEMES
        .iter()
        .position(|t| t.name.eq_ignore_ascii_case(name.trim()))
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lookup_themes_case_insensitive() {
        assert_eq!(theme_index_by_name("warm dark"), 0);
        assert_eq!(theme_index_by_name("Amber"), 1);
        assert_eq!(theme_index_by_name("PAPER"), 2);
        assert_eq!(theme_index_by_name("unknown"), 0);
    }
}
