use std::io::{self, Write};

use crossterm::cursor;
use crossterm::event::{self, Event, KeyCode, KeyModifiers};
use crossterm::style::{Print, ResetColor, SetForegroundColor};
use crossterm::terminal::{self, Clear, ClearType, ScrollUp};

use crate::commands::Command;
use crate::ui::style;

pub enum Result {
    Selected,
    Dismissed,
    Submitted,
}

pub fn run(buffer: &mut String, commands: &[Command]) -> anyhow::Result<Result> {
    let mut out = io::stdout();
    let mut selected: usize = 0;

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

        draw_menu(&mut out, &filtered, selected)?;

        if let Event::Key(key) = event::read()? {
            erase_menu(&mut out, filtered.len())?;

            match (key.code, key.modifiers) {
                (KeyCode::Char('c'), KeyModifiers::CONTROL) => {
                    buffer.clear();
                    return Ok(Result::Dismissed);
                }
                (KeyCode::Esc, _) => {
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
                        crossterm::execute!(
                            out,
                            cursor::MoveLeft(1),
                            Clear(ClearType::UntilNewLine),
                        )?;
                        selected = 0;
                    } else {
                        // Only "/" left, erase it and dismiss
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
                    crossterm::execute!(out, Print(c))?;
                    selected = 0;
                }
                _ => {}
            }
            out.flush()?;
        }
    }
}

fn draw_menu(out: &mut impl Write, items: &[(&Command, Option<&str>)], selected: usize) -> anyhow::Result<()> {
    let (col, row) = cursor::position()?;

    // Ensure enough room below for the menu (1 divider + N items)
    let lines_needed = (items.len() + 1) as u16;
    let (_, term_h) = terminal::size().unwrap_or((80, 24));
    let avail = term_h.saturating_sub(row + 1);
    if avail < lines_needed {
        let scroll = lines_needed - avail;
        // ScrollUp shifts viewport content up without corrupting visible lines
        crossterm::execute!(out, ScrollUp(scroll))?;
        let new_row = row.saturating_sub(scroll);
        crossterm::execute!(out, cursor::MoveTo(col, new_row))?;
    }

    let (_, origin_row) = cursor::position()?;

    // Skip past the bottom divider line
    crossterm::execute!(out, Print("\n\r"))?;

    for (i, (cmd, matched_alias)) in items.iter().enumerate() {
        crossterm::execute!(out, Print("\n\r"))?;

        // Build display name, e.g. "/exit" or "/exit (quit)"
        let display_name = match matched_alias {
            Some(alias) => format!("{} ({})", cmd.name, &alias[1..]),
            None => cmd.name.to_string(),
        };

        let desc_col = 24usize;
        let name_pad = desc_col.saturating_sub(display_name.len() + 3);

        if i == selected {
            // Selected: colored name and description
            crossterm::execute!(
                out,
                Print("  "),
                SetForegroundColor(style::HEADING),
                Print(&display_name),
                ResetColor,
            )?;
            write!(out, "{}", " ".repeat(name_pad))?;
            crossterm::execute!(
                out,
                SetForegroundColor(style::HEADING),
                Print(cmd.description),
                ResetColor,
            )?;
        } else {
            // Unselected: dark gray
            crossterm::execute!(
                out,
                Print("  "),
                SetForegroundColor(style::DIM),
                Print(&display_name),
            )?;
            write!(out, "{}", " ".repeat(name_pad))?;
            crossterm::execute!(
                out,
                Print(cmd.description),
                ResetColor,
            )?;
        }
    }

    crossterm::execute!(out, cursor::MoveTo(col, origin_row))?;
    out.flush()?;

    Ok(())
}

fn erase_menu(out: &mut impl Write, item_count: usize) -> anyhow::Result<()> {
    let (col, row) = cursor::position()?;

    // Skip past the bottom divider line
    crossterm::execute!(out, Print("\n\r"))?;

    for _ in 0..item_count {
        crossterm::execute!(
            out,
            Print("\n\r"),
            Clear(ClearType::CurrentLine),
        )?;
    }

    crossterm::execute!(out, cursor::MoveTo(col, row))?;
    out.flush()?;

    Ok(())
}
