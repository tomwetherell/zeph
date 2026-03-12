use std::io::{self, IsTerminal, Read, Write};
use std::time::Duration;

use crossterm::style::Color;

#[derive(Debug, Clone, Copy)]
pub enum ColorSupport {
    TrueColor,
    Ansi256,
    Basic,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Theme {
    Light,
    Dark,
}

#[derive(Debug, Clone, Copy)]
pub struct Palette {
    pub title: Color,
    pub heading: Color,
    pub dim: Color,
    pub dim_dark: Color,
    pub input_bg: Color,
    pub input_fg: Color,
}

pub fn detect_color_support() -> ColorSupport {
    if let Ok(val) = std::env::var("COLORTERM") {
        if val == "truecolor" || val == "24bit" {
            return ColorSupport::TrueColor;
        }
    }
    if let Ok(term) = std::env::var("TERM") {
        if term.contains("256color") {
            return ColorSupport::Ansi256;
        }
    }
    ColorSupport::Basic
}

pub fn detect_theme() -> Theme {
    if let Some(theme) = detect_theme_osc11() {
        return theme;
    }
    if let Some(theme) = detect_theme_colorfgbg() {
        return theme;
    }
    Theme::Dark
}

fn detect_theme_osc11() -> Option<Theme> {
    if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
        return None;
    }

    crossterm::terminal::enable_raw_mode().ok()?;
    let result = query_osc11();
    let _ = crossterm::terminal::disable_raw_mode();
    result
}

fn query_osc11() -> Option<Theme> {
    // Send OSC 11 query: "what is the background color?"
    let mut out = io::stdout();
    out.write_all(b"\x1b]11;?\x1b\\").ok()?;
    out.flush().ok()?;

    // Read response from stdin with a timeout. We use a dedicated thread
    // rather than crossterm's event system because crossterm's internal
    // reader would consume the raw OSC bytes before we can access them.
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut buf = [0u8; 64];
        let mut stdin = io::stdin();
        match stdin.read(&mut buf) {
            Ok(n) => { let _ = tx.send(buf[..n].to_vec()); }
            Err(_) => { let _ = tx.send(Vec::new()); }
        }
    });

    let bytes = rx.recv_timeout(Duration::from_millis(100)).ok()?;
    if bytes.is_empty() {
        return None;
    }

    let (r, g, b) = parse_osc11_response(&bytes)?;
    Some(luminance_to_theme(r, g, b))
}

fn detect_theme_colorfgbg() -> Option<Theme> {
    let val = std::env::var("COLORFGBG").ok()?;
    let bg = val.rsplit(';').next()?;
    let bg_num: u8 = bg.parse().ok()?;
    if bg_num >= 8 {
        Some(Theme::Light)
    } else {
        Some(Theme::Dark)
    }
}

fn parse_osc11_response(bytes: &[u8]) -> Option<(f64, f64, f64)> {
    let s = std::str::from_utf8(bytes).ok()?;
    let rgb_start = s.find("rgb:")?;
    let rgb_data = &s[rgb_start + 4..];

    // Terminate at ST (\x1b\\ or \x07)
    let end = rgb_data
        .find('\x1b')
        .or_else(|| rgb_data.find('\x07'))
        .unwrap_or(rgb_data.len());
    let rgb_data = &rgb_data[..end];

    let parts: Vec<&str> = rgb_data.split('/').collect();
    if parts.len() != 3 {
        return None;
    }

    let r = parse_hex_component(parts[0])?;
    let g = parse_hex_component(parts[1])?;
    let b = parse_hex_component(parts[2])?;
    Some((r, g, b))
}

fn parse_hex_component(s: &str) -> Option<f64> {
    let val = u16::from_str_radix(s, 16).ok()?;
    match s.len() {
        2 => Some(val as f64 / 0xFF as f64),
        4 => Some(val as f64 / 0xFFFF as f64),
        _ => None,
    }
}

fn luminance_to_theme(r: f64, g: f64, b: f64) -> Theme {
    let lum = 0.299 * r + 0.587 * g + 0.114 * b;
    if lum > 0.5 {
        Theme::Light
    } else {
        Theme::Dark
    }
}

impl Palette {
    pub fn new(support: ColorSupport, theme: Theme) -> Self {
        match (support, theme) {
            (ColorSupport::TrueColor, Theme::Light) => Self {
                title: Color::Black,
                heading: Color::Rgb {
                    r: 233,
                    g: 48,
                    b: 134,
                },
                dim: Color::Rgb {
                    r: 140,
                    g: 140,
                    b: 140,
                },
                dim_dark: Color::Rgb {
                    r: 110,
                    g: 110,
                    b: 110,
                },
                input_bg: Color::Rgb {
                    r: 240,
                    g: 240,
                    b: 240,
                },
                input_fg: Color::Black,
            },
            (ColorSupport::TrueColor, Theme::Dark) => Self {
                title: Color::White,
                heading: Color::Rgb {
                    r: 233,
                    g: 48,
                    b: 134,
                },
                dim: Color::Rgb {
                    r: 140,
                    g: 140,
                    b: 140,
                },
                dim_dark: Color::Rgb {
                    r: 110,
                    g: 110,
                    b: 110,
                },
                input_bg: Color::Rgb {
                    r: 50,
                    g: 50,
                    b: 50,
                },
                input_fg: Color::Rgb {
                    r: 220,
                    g: 220,
                    b: 220,
                },
            },
            (ColorSupport::Ansi256, Theme::Light) => Self {
                title: Color::Black,
                heading: Color::AnsiValue(198),
                dim: Color::AnsiValue(245),
                dim_dark: Color::AnsiValue(242),
                input_bg: Color::AnsiValue(255),
                input_fg: Color::Black,
            },
            (ColorSupport::Ansi256, Theme::Dark) => Self {
                title: Color::AnsiValue(255),
                heading: Color::AnsiValue(198),
                dim: Color::AnsiValue(245),
                dim_dark: Color::AnsiValue(242),
                input_bg: Color::AnsiValue(237),
                input_fg: Color::AnsiValue(252),
            },
            (ColorSupport::Basic, Theme::Light) => Self {
                title: Color::Black,
                heading: Color::Magenta,
                dim: Color::DarkGrey,
                dim_dark: Color::DarkGrey,
                input_bg: Color::White,
                input_fg: Color::Black,
            },
            (ColorSupport::Basic, Theme::Dark) => Self {
                title: Color::White,
                heading: Color::Magenta,
                dim: Color::DarkGrey,
                dim_dark: Color::DarkGrey,
                input_bg: Color::DarkGrey,
                input_fg: Color::White,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_osc11_4digit_white() {
        let response = b"\x1b]11;rgb:ffff/ffff/ffff\x1b\\";
        let (r, g, b) = parse_osc11_response(response).unwrap();
        assert!((r - 1.0).abs() < 0.001);
        assert!((g - 1.0).abs() < 0.001);
        assert!((b - 1.0).abs() < 0.001);
    }

    #[test]
    fn parse_osc11_4digit_black() {
        let response = b"\x1b]11;rgb:0000/0000/0000\x1b\\";
        let (r, g, b) = parse_osc11_response(response).unwrap();
        assert!(r.abs() < 0.001);
        assert!(g.abs() < 0.001);
        assert!(b.abs() < 0.001);
    }

    #[test]
    fn parse_osc11_2digit_hex() {
        let response = b"\x1b]11;rgb:ff/ff/ff\x07";
        let (r, g, b) = parse_osc11_response(response).unwrap();
        assert!((r - 1.0).abs() < 0.001);
        assert!((g - 1.0).abs() < 0.001);
        assert!((b - 1.0).abs() < 0.001);
    }

    #[test]
    fn parse_osc11_dark_background() {
        // Typical dark terminal: rgb:1c1c/1c1c/1c1c
        let response = b"\x1b]11;rgb:1c1c/1c1c/1c1c\x1b\\";
        let (r, g, b) = parse_osc11_response(response).unwrap();
        assert!(r < 0.2);
        assert_eq!(luminance_to_theme(r, g, b), Theme::Dark);
    }

    #[test]
    fn parse_osc11_light_background() {
        // Typical light terminal: rgb:ffff/ffff/ffff
        let response = b"\x1b]11;rgb:ffff/ffff/ffff\x1b\\";
        let (r, g, b) = parse_osc11_response(response).unwrap();
        assert_eq!(luminance_to_theme(r, g, b), Theme::Light);
    }

    #[test]
    fn parse_osc11_malformed_returns_none() {
        assert!(parse_osc11_response(b"garbage data").is_none());
        assert!(parse_osc11_response(b"\x1b]11;rgb:ffff/ffff\x1b\\").is_none());
        assert!(parse_osc11_response(b"").is_none());
    }

    #[test]
    fn luminance_threshold() {
        assert_eq!(luminance_to_theme(1.0, 1.0, 1.0), Theme::Light);
        assert_eq!(luminance_to_theme(0.0, 0.0, 0.0), Theme::Dark);
        // Mid-gray should be classified as dark (threshold is > 0.5)
        assert_eq!(luminance_to_theme(0.5, 0.5, 0.5), Theme::Dark);
    }

    #[test]
    fn colorfgbg_dark() {
        std::env::set_var("COLORFGBG", "15;0");
        assert_eq!(detect_theme_colorfgbg(), Some(Theme::Dark));
    }

    #[test]
    fn colorfgbg_light() {
        std::env::set_var("COLORFGBG", "0;15");
        assert_eq!(detect_theme_colorfgbg(), Some(Theme::Light));
    }

    #[test]
    fn colorfgbg_missing() {
        std::env::remove_var("COLORFGBG");
        assert_eq!(detect_theme_colorfgbg(), None);
    }
}
