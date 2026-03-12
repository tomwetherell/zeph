use crossterm::style::Color;

#[derive(Debug, Clone, Copy)]
pub enum ColorSupport {
    TrueColor,
    Ansi256,
    Basic,
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

impl Palette {
    pub fn new(support: ColorSupport) -> Self {
        match support {
            ColorSupport::TrueColor => Self {
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
            ColorSupport::Ansi256 => Self {
                title: Color::Black,
                heading: Color::AnsiValue(198),
                dim: Color::AnsiValue(245),
                dim_dark: Color::AnsiValue(242),
                input_bg: Color::AnsiValue(255),
                input_fg: Color::Black,
            },
            ColorSupport::Basic => Self {
                title: Color::Black,
                heading: Color::Magenta,
                dim: Color::DarkGrey,
                dim_dark: Color::DarkGrey,
                input_bg: Color::White,
                input_fg: Color::Black,
            },
        }
    }
}
