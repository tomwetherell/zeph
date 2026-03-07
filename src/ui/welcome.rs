use std::io::{self, Write};

use crossterm::style::{Attribute, Print, ResetColor, SetAttribute, SetForegroundColor};
use crossterm::terminal;

use super::style;

const VERSION: &str = env!("CARGO_PKG_VERSION");
const MIN_WIDTH: u16 = 60;
const MAX_WIDTH: u16 = 100;

pub fn render() -> anyhow::Result<()> {
    let (term_width, _) = terminal::size().unwrap_or((80, 24));
    let box_width = term_width.saturating_sub(4).clamp(MIN_WIDTH, MAX_WIDTH) as usize;
    let inner_width = box_width - 2; // excluding the two border chars

    let two_col = box_width >= 70;

    let mut out = io::stdout();

    // Top border: ┌─ zeph v0.1.0 ─────────┐
    let version_label = format!(" zeph v{VERSION} ");
    let remaining = inner_width.saturating_sub(version_label.len());
    crossterm::execute!(
        out,
        SetForegroundColor(style::BORDER),
        Print("┌─"),
        SetForegroundColor(style::TITLE),
        SetAttribute(Attribute::Bold),
        Print(&version_label),
        SetAttribute(Attribute::Reset),
        SetForegroundColor(style::BORDER),
        Print("─".repeat(remaining.saturating_sub(1))),
        Print("┐\n"),
        ResetColor,
    )?;

    if two_col {
        render_two_col(&mut out, inner_width)?;
    } else {
        render_single_col(&mut out, inner_width)?;
    }

    // Bottom border
    crossterm::execute!(
        out,
        SetForegroundColor(style::BORDER),
        Print("└"),
        Print("─".repeat(inner_width)),
        Print("┘\n"),
        ResetColor,
    )?;

    Ok(())
}

fn render_two_col(out: &mut impl Write, inner_width: usize) -> anyhow::Result<()> {
    let left_width = inner_width / 2;
    let right_width = inner_width - left_width - 1; // -1 for the vertical divider

    let left_lines = build_left_panel(left_width);
    let right_lines = build_right_panel(right_width);
    let row_count = left_lines.len().max(right_lines.len());

    for i in 0..row_count {
        let left = left_lines
            .get(i)
            .map(|s| s.as_str())
            .unwrap_or("");
        let right = right_lines
            .get(i)
            .map(|s| s.as_str())
            .unwrap_or("");

        let left_pad = left_width.saturating_sub(visible_len(left));
        let right_pad = right_width.saturating_sub(visible_len(right));

        crossterm::execute!(
            out,
            SetForegroundColor(style::BORDER),
            Print("│"),
            ResetColor,
        )?;
        write!(out, "{left}{}", " ".repeat(left_pad))?;
        crossterm::execute!(
            out,
            SetForegroundColor(style::BORDER),
            Print("│"),
            ResetColor,
        )?;
        write!(out, "{right}{}", " ".repeat(right_pad))?;
        crossterm::execute!(
            out,
            SetForegroundColor(style::BORDER),
            Print("│\n"),
            ResetColor,
        )?;
    }

    Ok(())
}

fn render_single_col(out: &mut impl Write, inner_width: usize) -> anyhow::Result<()> {
    let mut lines = build_left_panel(inner_width);
    lines.push(String::new());
    lines.extend(build_right_panel(inner_width));

    for line in &lines {
        let pad = inner_width.saturating_sub(visible_len(line));
        crossterm::execute!(
            out,
            SetForegroundColor(style::BORDER),
            Print("│"),
            ResetColor,
        )?;
        write!(out, "{line}{}", " ".repeat(pad))?;
        crossterm::execute!(
            out,
            SetForegroundColor(style::BORDER),
            Print("│\n"),
            ResetColor,
        )?;
    }

    Ok(())
}

fn build_left_panel(_width: usize) -> Vec<String> {
    vec![
        String::new(),
        "   Welcome to zeph!".to_string(),
        String::new(),
    ]
}

fn build_right_panel(_width: usize) -> Vec<String> {
    vec![
        format!("\x1b[1;38;2;230;140;100m Tips for getting started\x1b[0m"),
        format!("\x1b[38;5;240m ─────────────────────────\x1b[0m"),
        " Type /help for commands".to_string(),
        " Type /quit to exit".to_string(),
        " Type / to see all options".to_string(),
        String::new(),
    ]
}

/// Compute the visible length of a string, ignoring ANSI escape sequences.
fn visible_len(s: &str) -> usize {
    let mut len = 0;
    let mut in_escape = false;
    for c in s.chars() {
        if in_escape {
            if c.is_ascii_alphabetic() {
                in_escape = false;
            }
        } else if c == '\x1b' {
            in_escape = true;
        } else {
            len += 1;
        }
    }
    len
}
