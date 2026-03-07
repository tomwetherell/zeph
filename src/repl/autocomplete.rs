use std::io::{self, Write};

use crossterm::cursor;
use crossterm::event::{self, Event, KeyCode, KeyModifiers};
use crossterm::style::{
    Attribute, Color, Print, ResetColor, SetAttribute, SetBackgroundColor, SetForegroundColor,
};
use crossterm::terminal::{self, Clear, ClearType};

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
        let filtered: Vec<&Command> = commands
            .iter()
            .filter(|c| c.name.starts_with(buffer.as_str()))
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
                    if let Some(cmd) = filtered.get(selected) {
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

fn draw_menu(out: &mut impl Write, items: &[&Command], selected: usize) -> anyhow::Result<()> {
    let (term_width, _) = terminal::size().unwrap_or((80, 24));

    // Save cursor position
    crossterm::execute!(out, cursor::SavePosition)?;

    for (i, cmd) in items.iter().enumerate() {
        crossterm::execute!(out, Print("\n\r"))?;

        if i == selected {
            crossterm::execute!(
                out,
                SetBackgroundColor(Color::Rgb {
                    r: 60,
                    g: 60,
                    b: 60
                }),
            )?;
        }

        let desc_col = 24usize;
        let name_pad = desc_col.saturating_sub(cmd.name.len() + 3);

        crossterm::execute!(
            out,
            Print("  "),
            SetForegroundColor(style::HEADING),
            SetAttribute(Attribute::Bold),
            Print(cmd.name),
            SetAttribute(Attribute::Reset),
        )?;

        if i == selected {
            crossterm::execute!(
                out,
                SetBackgroundColor(Color::Rgb {
                    r: 60,
                    g: 60,
                    b: 60
                }),
            )?;
        }

        write!(out, "{}", " ".repeat(name_pad))?;

        crossterm::execute!(
            out,
            SetForegroundColor(style::DIM),
            Print(cmd.description),
            ResetColor,
        )?;

        // Fill rest of line if selected (for background highlight)
        if i == selected {
            let used = 2 + cmd.name.len() + name_pad + cmd.description.len();
            let remaining = (term_width as usize).saturating_sub(used);
            write!(out, "{}", " ".repeat(remaining))?;
            crossterm::execute!(out, ResetColor)?;
        }
    }

    // Restore cursor position
    crossterm::execute!(out, cursor::RestorePosition)?;
    out.flush()?;

    Ok(())
}

fn erase_menu(out: &mut impl Write, item_count: usize) -> anyhow::Result<()> {
    crossterm::execute!(out, cursor::SavePosition)?;

    for _ in 0..item_count {
        crossterm::execute!(
            out,
            Print("\n\r"),
            Clear(ClearType::CurrentLine),
        )?;
    }

    crossterm::execute!(out, cursor::RestorePosition)?;
    out.flush()?;

    Ok(())
}
