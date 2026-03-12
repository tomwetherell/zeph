use std::io;

use crossterm::style::{Attribute, Print, ResetColor, SetAttribute, SetForegroundColor};

use super::style::Palette;

const VERSION: &str = env!("CARGO_PKG_VERSION");

pub fn render(store_path: &str, palette: &Palette) -> anyhow::Result<()> {
    let mut out = io::stdout();

    let display_path = shorten_home(store_path);

    // "Zeph" bold black, version in dim
    crossterm::execute!(
        out,
        Print("\n"),
        SetForegroundColor(palette.title),
        SetAttribute(Attribute::Bold),
        Print("  Zeph "),
        SetAttribute(Attribute::Reset),
        SetForegroundColor(palette.dim),
        Print(format!("v{VERSION}")),
        ResetColor,
        Print("\n"),
    )?;

    // Store path in dim
    crossterm::execute!(
        out,
        SetForegroundColor(palette.dim),
        Print(format!("  {display_path}")),
        ResetColor,
        Print("\n\n"),
    )?;

    Ok(())
}

fn shorten_home(path: &str) -> String {
    if let Ok(home) = std::env::var("HOME") {
        if let Some(rest) = path.strip_prefix(&home) {
            return format!("~{rest}");
        }
    }
    path.to_string()
}
