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

pub fn read_input(commands: &[Command]) -> anyhow::Result<Input> {
    let mut out = io::stdout();

    // Print prompt
    crossterm::execute!(
        out,
        SetForegroundColor(style::HEADING),
        Print("> "),
        ResetColor,
    )?;
    out.flush()?;

    let _guard = RawModeGuard::enable()?;
    let mut buffer = String::new();

    loop {
        if let Event::Key(key) = event::read()? {
            match (key.code, key.modifiers) {
                (KeyCode::Char('c'), KeyModifiers::CONTROL) => {
                    crossterm::execute!(out, Print("\n"))?;
                    return Ok(Input::Quit);
                }
                (KeyCode::Enter, _) => {
                    crossterm::execute!(out, Print("\n"))?;
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
                            crossterm::execute!(out, Print("\n"))?;
                            return Ok(Input::Command(buffer));
                        }
                        autocomplete::Result::Dismissed => {
                            // User pressed Esc, buffer may have been cleared
                            if buffer.is_empty() {
                                // Erase the prompt line and reprint
                                crossterm::execute!(
                                    out,
                                    Print("\r"),
                                    Clear(ClearType::CurrentLine),
                                )?;
                                drop(_guard);
                                return Ok(Input::Empty);
                            }
                        }
                        autocomplete::Result::Submitted => {
                            crossterm::execute!(out, Print("\n"))?;
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
