use std::io::{self, Write};

use crossterm::cursor;
use crossterm::event::{self, Event, KeyCode, KeyModifiers};
use crossterm::style::{Print, ResetColor, SetForegroundColor};
use crossterm::terminal::{self, Clear, ClearType};

use crate::commands::Command;
use crate::ui::style;

use super::autocomplete;

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

pub fn print_divider(out: &mut impl Write) -> anyhow::Result<()> {
    let (term_width, _) = terminal::size().unwrap_or((80, 24));
    let line = "─".repeat(term_width as usize);
    crossterm::execute!(
        out,
        SetForegroundColor(style::DIM),
        Print(&line),
        Print("\n"),
        ResetColor,
    )?;
    Ok(())
}

pub fn read_input(commands: &[Command]) -> anyhow::Result<Input> {
    let mut out = io::stdout();

    // Print divider above prompt
    print_divider(&mut out)?;

    // Print prompt
    crossterm::execute!(
        out,
        SetForegroundColor(crossterm::style::Color::Black),
        Print("❯ "),
        ResetColor,
    )?;

    // Print bottom divider, then move cursor back up to prompt line
    crossterm::execute!(out, Print("\n"))?;
    print_divider(&mut out)?;
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
                    crossterm::execute!(out, cursor::MoveDown(1), Print("\r\n"))?;
                    return Ok(Input::Quit);
                }
                (KeyCode::Enter, _) => {
                    crossterm::execute!(out, cursor::MoveDown(1), Print("\r\n"))?;
                    return if buffer.is_empty() {
                        Ok(Input::Empty)
                    } else {
                        Ok(Input::Command(buffer))
                    };
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
                    match autocomplete::run(&mut buffer, commands)? {
                        autocomplete::Result::Selected => {
                            // Command was filled into buffer, submit it
                            crossterm::execute!(out, cursor::MoveDown(1), Print("\r\n"))?;
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
                            crossterm::execute!(out, cursor::MoveDown(1), Print("\r\n"))?;
                            return Ok(Input::Command(buffer));
                        }
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
