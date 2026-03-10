use std::io::{self, Write};

use crossterm::cursor;
use crossterm::event::{self, Event, KeyCode, KeyModifiers};
use crossterm::style::{Print, ResetColor, SetForegroundColor};
use crossterm::terminal::{self, Clear, ClearType, ScrollUp};

use crate::commands::summary::friendly_dtype;
use crate::ui::style;
use zeph::zarr::metadata::ArrayMeta;

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

/// Run the variable picker. Returns `Some(index)` into the arrays list, or `None` if cancelled.
pub fn run(arrays: &[ArrayMeta]) -> anyhow::Result<Option<usize>> {
    let mut out = io::stdout();
    let mut selected: usize = 0;
    let mut filter = String::new();
    let mut prev_count: usize = 0;

    // Print the prompt label
    crossterm::execute!(
        out,
        Print("\n"),
        SetForegroundColor(style::HEADING),
        Print("  Select a variable: "),
        ResetColor,
    )?;

    // Ensure enough room below for the menu
    let (col, row) = cursor::position()?;
    let lines_needed = (arrays.len() + 1) as u16;
    let (_, term_h) = terminal::size().unwrap_or((80, 24));
    let avail = term_h.saturating_sub(row + 1);
    if avail < lines_needed {
        let scroll = lines_needed - avail;
        crossterm::execute!(out, ScrollUp(scroll))?;
        crossterm::execute!(out, cursor::MoveTo(col, row.saturating_sub(scroll)))?;
    }

    let _guard = RawModeGuard::enable()?;

    loop {
        // Build filtered list with original indices
        let filtered: Vec<(usize, &ArrayMeta)> = arrays
            .iter()
            .enumerate()
            .filter(|(_, a)| {
                filter.is_empty() || a.name.contains(filter.as_str())
            })
            .collect();

        if selected >= filtered.len() {
            selected = filtered.len().saturating_sub(1);
        }

        draw_picker(&mut out, &filtered, selected, prev_count)?;
        prev_count = filtered.len();

        if let Event::Key(key) = event::read()? {
            match (key.code, key.modifiers) {
                (KeyCode::Char('c'), KeyModifiers::CONTROL) | (KeyCode::Esc, _) => {
                    clear_picker(&mut out, prev_count)?;
                    // Erase the prompt label and filter text
                    erase_prompt(&mut out, &filter)?;
                    return Ok(None);
                }
                (KeyCode::Enter, _) => {
                    if let Some(&(orig_idx, _)) = filtered.get(selected) {
                        clear_picker(&mut out, prev_count)?;
                        // Clear the prompt line
                        crossterm::execute!(
                            out,
                            Print("\r"),
                            Clear(ClearType::CurrentLine),
                        )?;
                        return Ok(Some(orig_idx));
                    }
                    // No matches — ignore enter
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
                    if filtered.is_empty() {
                        // nothing
                    } else if selected + 1 < filtered.len() {
                        selected += 1;
                    } else {
                        selected = 0;
                    }
                }
                (KeyCode::Backspace, _) => {
                    if !filter.is_empty() {
                        filter.pop();
                        crossterm::queue!(
                            out,
                            cursor::MoveLeft(1),
                            Clear(ClearType::UntilNewLine),
                        )?;
                        selected = 0;
                    }
                }
                (KeyCode::Char(c), _) => {
                    filter.push(c);
                    crossterm::queue!(out, Print(c))?;
                    selected = 0;
                }
                _ => {}
            }
        }
    }
}

fn format_item(arr: &ArrayMeta) -> String {
    let dims = if arr.dims.is_empty() {
        String::new()
    } else {
        let parts: Vec<String> = arr
            .dims
            .iter()
            .zip(arr.shape.iter())
            .map(|(d, s)| format!("{d}: {s}"))
            .collect();
        format!("  ({})", parts.join(", "))
    };
    let dtype = friendly_dtype(&arr.dtype);
    format!("{}{dims}  {dtype}", arr.name)
}

fn draw_picker(
    out: &mut impl Write,
    items: &[(usize, &ArrayMeta)],
    selected: usize,
    prev_count: usize,
) -> anyhow::Result<()> {
    crossterm::queue!(out, cursor::SavePosition)?;

    for (i, (_, arr)) in items.iter().enumerate() {
        crossterm::queue!(out, Print("\n\r"), Clear(ClearType::CurrentLine))?;

        let text = format_item(arr);
        if i == selected {
            crossterm::queue!(
                out,
                Print("    "),
                SetForegroundColor(style::HEADING),
                Print(&text),
                ResetColor,
            )?;
        } else {
            crossterm::queue!(
                out,
                Print("    "),
                SetForegroundColor(style::DIM),
                Print(&text),
                ResetColor,
            )?;
        }
    }

    // Clear leftover lines from previous render
    for _ in items.len()..prev_count {
        crossterm::queue!(out, Print("\n\r"), Clear(ClearType::CurrentLine))?;
    }

    crossterm::queue!(out, cursor::RestorePosition)?;
    out.flush()?;

    Ok(())
}

fn clear_picker(out: &mut impl Write, item_count: usize) -> anyhow::Result<()> {
    crossterm::queue!(out, cursor::SavePosition)?;
    for _ in 0..item_count {
        crossterm::queue!(out, Print("\n\r"), Clear(ClearType::CurrentLine))?;
    }
    crossterm::queue!(out, cursor::RestorePosition)?;
    out.flush()?;
    Ok(())
}

fn erase_prompt(out: &mut impl Write, _filter: &str) -> anyhow::Result<()> {
    crossterm::execute!(out, Print("\r"), Clear(ClearType::CurrentLine))?;
    Ok(())
}
