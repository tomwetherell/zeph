use std::io::{self, Write};

use crossterm::cursor;
use crossterm::event::{self, Event, KeyCode, KeyModifiers};
use crossterm::style::{Print, ResetColor, SetForegroundColor};
use crossterm::terminal::{self, Clear, ClearType, ScrollUp};

use crate::commands::Command;
use crate::ui::style::Palette;

pub enum Result {
    Selected,
    Dismissed,
    Submitted,
}

pub fn run(buffer: &mut String, commands: &[Command], palette: &Palette) -> anyhow::Result<Result> {
    let mut out = io::stdout();
    let mut selected: usize = 0;
    let mut prev_count: usize = 0;

    // Ensure enough room below for the menu once, before the loop.
    // The first iteration matches all commands (buffer is "/"), so this
    // covers the maximum menu size.
    let (col, row) = cursor::position()?;
    let lines_needed = (commands.len() + 1) as u16;
    let (_, term_h) = terminal::size().unwrap_or((80, 24));
    let avail = term_h.saturating_sub(row + 1);
    if avail < lines_needed {
        let scroll = lines_needed - avail;
        crossterm::execute!(out, ScrollUp(scroll))?;
        crossterm::execute!(out, cursor::MoveTo(col, row.saturating_sub(scroll)))?;
    }

    loop {
        let filtered: Vec<(&Command, Option<&str>)> = commands
            .iter()
            .filter_map(|c| {
                if c.name.starts_with(buffer.as_str()) {
                    Some((c, None))
                } else if let Some(alias) = c.aliases.iter().find(|a| a.starts_with(buffer.as_str())) {
                    Some((c, Some(*alias)))
                } else {
                    None
                }
            })
            .collect();

        draw_menu(&mut out, &filtered, selected, prev_count, palette)?;
        prev_count = filtered.len();

        if let Event::Key(key) = event::read()? {
            match (key.code, key.modifiers) {
                (KeyCode::Char('c'), KeyModifiers::CONTROL) => {
                    clear_menu(&mut out, prev_count)?;
                    buffer.clear();
                    return Ok(Result::Dismissed);
                }
                (KeyCode::Esc, _) => {
                    clear_menu(&mut out, prev_count)?;
                    // Clear buffer and erase typed text
                    let len = buffer.len() as u16;
                    if len > 0 {
                        crossterm::execute!(
                            out,
                            cursor::MoveLeft(len),
                            Clear(ClearType::UntilNewLine),
                        )?;
                    }
                    buffer.clear();
                    return Ok(Result::Dismissed);
                }
                (KeyCode::Enter, _) => {
                    clear_menu(&mut out, prev_count)?;
                    if let Some((cmd, _)) = filtered.get(selected) {
                        // Replace buffer with selected command
                        let old_len = buffer.len() as u16;
                        if old_len > 0 {
                            crossterm::execute!(
                                out,
                                cursor::MoveLeft(old_len),
                                Clear(ClearType::UntilNewLine),
                            )?;
                        }
                        *buffer = cmd.name.to_string();
                        crossterm::execute!(out, Print(buffer.as_str()))?;
                        return Ok(Result::Selected);
                    }
                    return Ok(Result::Submitted);
                }
                (KeyCode::Up, _) => {
                    if selected > 0 {
                        selected -= 1;
                    }
                }
                (KeyCode::Down, _) => {
                    if selected + 1 < filtered.len() {
                        selected += 1;
                    }
                }
                (KeyCode::Tab, _) => {
                    if selected + 1 < filtered.len() {
                        selected += 1;
                    } else {
                        selected = 0;
                    }
                }
                (KeyCode::Backspace, _) => {
                    if buffer.len() > 1 {
                        buffer.pop();
                        crossterm::queue!(
                            out,
                            cursor::MoveLeft(1),
                            Clear(ClearType::UntilNewLine),
                        )?;
                        selected = 0;
                    } else {
                        // Only "/" left, erase it and dismiss
                        clear_menu(&mut out, prev_count)?;
                        crossterm::execute!(
                            out,
                            cursor::MoveLeft(1),
                            Clear(ClearType::UntilNewLine),
                        )?;
                        buffer.clear();
                        return Ok(Result::Dismissed);
                    }
                }
                (KeyCode::Char(c), _) => {
                    buffer.push(c);
                    crossterm::queue!(out, Print(c))?;
                    selected = 0;
                }
                _ => {}
            }
        }
    }
}

/// Draw the menu, overwriting any previous content in place.
/// Clears each line before writing to handle shrinking content,
/// and clears leftover lines when the item count decreases.
fn draw_menu(out: &mut impl Write, items: &[(&Command, Option<&str>)], selected: usize, prev_count: usize, palette: &Palette) -> anyhow::Result<()> {
    crossterm::queue!(out, cursor::SavePosition)?;

    // Skip past the bottom divider line
    crossterm::queue!(out, Print("\n\r"))?;

    for (i, (cmd, matched_alias)) in items.iter().enumerate() {
        crossterm::queue!(out, Print("\n\r"), Clear(ClearType::CurrentLine))?;

        // Build display name, e.g. "/exit" or "/exit (quit)"
        let display_name = match matched_alias {
            Some(alias) => format!("{} ({})", cmd.name, &alias[1..]),
            None => cmd.name.to_string(),
        };

        let desc_col = 24usize;
        let name_pad = desc_col.saturating_sub(display_name.len() + 3);

        if i == selected {
            // Selected: colored name and description
            crossterm::queue!(
                out,
                Print("  "),
                SetForegroundColor(palette.heading),
                Print(&display_name),
                ResetColor,
            )?;
            write!(out, "{}", " ".repeat(name_pad))?;
            crossterm::queue!(
                out,
                SetForegroundColor(palette.heading),
                Print(cmd.description),
                ResetColor,
            )?;
        } else {
            // Unselected: dark gray
            crossterm::queue!(
                out,
                Print("  "),
                SetForegroundColor(palette.dim),
                Print(&display_name),
            )?;
            write!(out, "{}", " ".repeat(name_pad))?;
            crossterm::queue!(
                out,
                Print(cmd.description),
                ResetColor,
            )?;
        }
    }

    // Clear any leftover lines from previous render
    for _ in items.len()..prev_count {
        crossterm::queue!(out, Print("\n\r"), Clear(ClearType::CurrentLine))?;
    }

    crossterm::queue!(out, cursor::RestorePosition)?;
    out.flush()?;

    Ok(())
}

/// Clear the menu completely (used only on exit paths).
fn clear_menu(out: &mut impl Write, item_count: usize) -> anyhow::Result<()> {
    crossterm::queue!(out, cursor::SavePosition, Print("\n\r"))?;
    for _ in 0..item_count {
        crossterm::queue!(out, Print("\n\r"), Clear(ClearType::CurrentLine))?;
    }
    crossterm::queue!(out, cursor::RestorePosition)?;
    out.flush()?;

    Ok(())
}
