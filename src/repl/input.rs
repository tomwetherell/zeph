use std::io::{self, Write};

use crossterm::cursor;
use crossterm::event::{self, Event, KeyCode, KeyModifiers};
use crossterm::style::{Print, ResetColor, SetBackgroundColor, SetForegroundColor};
use crossterm::terminal::{self, Clear, ClearType};

use crate::commands::Command;
use crate::ui::style::Palette;

use super::autocomplete;
use super::history::History;

pub enum Input {
    Command(String),
    Quit,
    Empty,
}

struct RawModeGuard;

impl RawModeGuard {
    fn enable() -> anyhow::Result<Self> {
        terminal::enable_raw_mode()?;
        Ok(Self)
    }
}

impl Drop for RawModeGuard {
    fn drop(&mut self) {
        let _ = terminal::disable_raw_mode();
    }
}

pub fn print_divider(out: &mut impl Write, palette: &Palette) -> anyhow::Result<()> {
    let (term_width, _) = terminal::size().unwrap_or((80, 24));
    let line = "─".repeat(term_width as usize);
    crossterm::execute!(
        out,
        SetForegroundColor(palette.dim),
        Print(&line),
        Print("\n"),
        ResetColor,
    )?;
    Ok(())
}

/// After a command is submitted, rewrite the 3-line prompt block
/// (top divider, input line, bottom divider) to show the command
/// on a gray background with no dividers.
/// Cursor must be on the input line when called.
fn confirm_prompt(out: &mut impl Write, buffer: &str, palette: &Palette) -> anyhow::Result<()> {
    let (term_width, _) = terminal::size().unwrap_or((80, 24));

    // Move up to top divider and clear it
    crossterm::execute!(out, cursor::MoveUp(1), Print("\r"), Clear(ClearType::CurrentLine))?;

    // Move down to input line, clear and rewrite with gray background
    crossterm::execute!(out, cursor::MoveDown(1), Print("\r"), Clear(ClearType::CurrentLine))?;
    let cmd_text = format!(" {buffer}");
    let arrow_pad = 1; // "❯" is 1 char wide
    let pad = (term_width as usize).saturating_sub(arrow_pad + cmd_text.chars().count());
    crossterm::execute!(
        out,
        SetBackgroundColor(palette.input_bg),
        SetForegroundColor(palette.dim),
        Print("❯"),
        SetForegroundColor(palette.input_fg),
        Print(&cmd_text),
        Print(" ".repeat(pad)),
        ResetColor,
    )?;

    // Move down to bottom divider and clear it
    crossterm::execute!(out, Print("\n\r"), Clear(ClearType::CurrentLine))?;

    Ok(())
}

/// Replace the visible input text and buffer with new content.
fn replace_buffer(out: &mut impl Write, buffer: &mut String, new: &str) -> anyhow::Result<()> {
    // Move cursor to start of input (after "❯ ")
    crossterm::execute!(
        out,
        Print("\r"),
        cursor::MoveRight(2),
        Clear(ClearType::UntilNewLine),
        Print(new),
    )?;
    buffer.clear();
    buffer.push_str(new);
    Ok(())
}

pub fn read_input(commands: &[Command], palette: &Palette, history: &mut History) -> anyhow::Result<Input> {
    let mut out = io::stdout();

    // Print divider above prompt
    print_divider(&mut out, palette)?;

    // Print prompt
    crossterm::execute!(
        out,
        SetForegroundColor(palette.input_fg),
        Print("❯ "),
        ResetColor,
    )?;

    // Print bottom divider, then move cursor back up to prompt line
    crossterm::execute!(out, Print("\n"))?;
    print_divider(&mut out, palette)?;
    crossterm::execute!(
        out,
        cursor::MoveUp(2),
        Print("\r"),
        cursor::MoveRight(2), // position after "❯ "
    )?;
    out.flush()?;

    let _guard = RawModeGuard::enable()?;
    let mut buffer = String::new();

    loop {
        if let Event::Key(key) = event::read()? {
            match (key.code, key.modifiers) {
                (KeyCode::Char('c'), KeyModifiers::CONTROL) => {
                    history.reset();
                    crossterm::execute!(out, cursor::MoveDown(1), Print("\r\n"))?;
                    return Ok(Input::Quit);
                }
                (KeyCode::Enter, _) => {
                    history.reset();
                    if buffer.is_empty() {
                        crossterm::execute!(out, cursor::MoveDown(1), Print("\r\n"))?;
                        return Ok(Input::Empty);
                    } else {
                        confirm_prompt(&mut out, &buffer, palette)?;
                        return Ok(Input::Command(buffer));
                    }
                }
                (KeyCode::Backspace, _) => {
                    if !buffer.is_empty() {
                        buffer.pop();
                        crossterm::execute!(
                            out,
                            cursor::MoveLeft(1),
                            Clear(ClearType::UntilNewLine),
                        )?;
                    }
                }
                (KeyCode::Char('/'), _) if buffer.is_empty() => {
                    buffer.push('/');
                    crossterm::execute!(out, Print("/"))?;
                    out.flush()?;

                    // Enter autocomplete mode
                    match autocomplete::run(&mut buffer, commands, palette)? {
                        autocomplete::Result::Selected => {
                            confirm_prompt(&mut out, &buffer, palette)?;
                            return Ok(Input::Command(buffer));
                        }
                        autocomplete::Result::Dismissed => {
                            if buffer.is_empty() {
                                // User deleted everything — erase the prompt text
                                // and stay in the input loop
                                crossterm::execute!(
                                    out,
                                    Print("\r"),
                                    cursor::MoveRight(2), // past "❯ "
                                    Clear(ClearType::UntilNewLine),
                                )?;
                            }
                            // Continue reading input
                        }
                        autocomplete::Result::Submitted => {
                            confirm_prompt(&mut out, &buffer, palette)?;
                            return Ok(Input::Command(buffer));
                        }
                    }
                }
                (KeyCode::Up, _) => {
                    if let Some(entry) = history.up() {
                        let entry = entry.to_string();
                        replace_buffer(&mut out, &mut buffer, &entry)?;
                    }
                }
                (KeyCode::Down, _) => {
                    if let Some(entry) = history.down() {
                        let entry = entry.to_string();
                        replace_buffer(&mut out, &mut buffer, &entry)?;
                    }
                }
                (KeyCode::Char(c), _) => {
                    buffer.push(c);
                    crossterm::execute!(out, Print(c))?;
                }
                _ => {}
            }
            out.flush()?;
        }
    }
}
