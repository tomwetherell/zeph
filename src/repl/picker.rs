use std::io::{self, Write};

use crossterm::cursor;
use crossterm::event::{self, Event, KeyCode, KeyModifiers};
use crossterm::style::{Print, ResetColor, SetForegroundColor};
use crossterm::terminal::{self, Clear, ClearType, ScrollUp};

use crate::commands::summary::friendly_dtype;
use crate::ui::style::Palette;
use zeph::zarr::metadata::ArrayMeta;

const WINDOW_SIZE: usize = 10;

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
pub fn run(arrays: &[ArrayMeta], palette: &Palette) -> anyhow::Result<Option<usize>> {
    let mut out = io::stdout();
    let mut selected: usize = 0;
    let mut filter = String::new();
    let mut prev_lines: usize = 0;
    let mut viewport_start: usize = 0;

    // Print the prompt label
    crossterm::execute!(
        out,
        Print("\n"),
        Print("  Select a variable (type to filter): "),
    )?;

    // Ensure enough room below for the menu
    // items + separator + up to 2 indicator lines
    let (col, row) = cursor::position()?;
    let lines_needed = (arrays.len().min(WINDOW_SIZE) + 3) as u16;
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

        viewport_start = compute_viewport(selected, viewport_start, filtered.len(), WINDOW_SIZE);

        prev_lines = draw_picker(&mut out, &filtered, selected, viewport_start, WINDOW_SIZE, prev_lines, palette)?;

        if let Event::Key(key) = event::read()? {
            match (key.code, key.modifiers) {
                (KeyCode::Char('c'), KeyModifiers::CONTROL) | (KeyCode::Esc, _) => {
                    clear_picker(&mut out, prev_lines)?;
                    // Erase the prompt label and filter text
                    erase_prompt(&mut out, &filter)?;
                    return Ok(None);
                }
                (KeyCode::Enter, _) => {
                    if let Some(&(orig_idx, _)) = filtered.get(selected) {
                        clear_picker(&mut out, prev_lines)?;
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
                        viewport_start = 0;
                    }
                }
                (KeyCode::Char(c), _) => {
                    filter.push(c);
                    crossterm::queue!(out, Print(c))?;
                    selected = 0;
                    viewport_start = 0;
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
    viewport_start: usize,
    window_size: usize,
    prev_lines: usize,
    palette: &Palette,
) -> anyhow::Result<usize> {
    crossterm::queue!(out, cursor::SavePosition)?;

    let mut lines_rendered: usize = 0;

    if !items.is_empty() {
        let visible = items.len().min(window_size);
        let viewport_end = (viewport_start + visible).min(items.len());

        // Separator line
        crossterm::queue!(out, Print("\n\r"))?;
        lines_rendered += 1;

        // "↑ N more" indicator
        if viewport_start > 0 {
            crossterm::queue!(
                out,
                Print("\n\r"),
                Clear(ClearType::CurrentLine),
                Print("    "),
                SetForegroundColor(palette.dim),
                Print(format!("↑ {} more", viewport_start)),
                ResetColor,
            )?;
            lines_rendered += 1;
        }

        // Visible items
        for i in viewport_start..viewport_end {
            let (_, arr) = &items[i];
            crossterm::queue!(out, Print("\n\r"), Clear(ClearType::CurrentLine))?;
            lines_rendered += 1;

            let text = format_item(arr);
            if i == selected {
                crossterm::queue!(
                    out,
                    SetForegroundColor(palette.heading),
                    Print("  ❯ "),
                    Print(&text),
                    ResetColor,
                )?;
            } else {
                crossterm::queue!(
                    out,
                    Print("    "),
                    SetForegroundColor(palette.dim),
                    Print(&text),
                    ResetColor,
                )?;
            }
        }

        // "↓ N more" indicator
        let remaining = items.len().saturating_sub(viewport_end);
        if remaining > 0 {
            crossterm::queue!(
                out,
                Print("\n\r"),
                Clear(ClearType::CurrentLine),
                Print("    "),
                SetForegroundColor(palette.dim),
                Print(format!("↓ {} more", remaining)),
                ResetColor,
            )?;
            lines_rendered += 1;
        }
    }

    // Clear leftover lines from previous render
    for _ in lines_rendered..prev_lines {
        crossterm::queue!(out, Print("\n\r"), Clear(ClearType::CurrentLine))?;
    }

    crossterm::queue!(out, cursor::RestorePosition)?;
    out.flush()?;

    Ok(lines_rendered)
}

fn clear_picker(out: &mut impl Write, prev_lines: usize) -> anyhow::Result<()> {
    crossterm::queue!(out, cursor::SavePosition)?;
    for _ in 0..prev_lines {
        crossterm::queue!(out, Print("\n\r"), Clear(ClearType::CurrentLine))?;
    }
    crossterm::queue!(out, cursor::RestorePosition)?;
    out.flush()?;
    Ok(())
}

fn compute_viewport(
    selected: usize,
    current_start: usize,
    total_items: usize,
    window_size: usize,
) -> usize {
    let visible = total_items.min(window_size);
    if total_items <= window_size {
        return 0;
    }
    let mut start = current_start;
    if selected < start {
        start = selected;
    }
    if selected >= start + visible {
        start = selected + 1 - visible;
    }
    start.min(total_items.saturating_sub(visible))
}

fn erase_prompt(out: &mut impl Write, _filter: &str) -> anyhow::Result<()> {
    crossterm::execute!(out, Print("\r"), Clear(ClearType::CurrentLine))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn viewport_fits_in_window() {
        // 5 items, window 10 — viewport stays 0
        assert_eq!(compute_viewport(0, 0, 5, 10), 0);
        assert_eq!(compute_viewport(4, 0, 5, 10), 0);
    }

    #[test]
    fn viewport_scrolls_down() {
        // 20 items, window 10, select item 12
        assert_eq!(compute_viewport(12, 0, 20, 10), 3);
    }

    #[test]
    fn viewport_scrolls_up() {
        // Viewport at 5, select item 2
        assert_eq!(compute_viewport(2, 5, 20, 10), 2);
    }

    #[test]
    fn viewport_resets_on_wrap_to_zero() {
        // Tab wraps to 0 from the end
        assert_eq!(compute_viewport(0, 10, 20, 10), 0);
    }

    #[test]
    fn viewport_clamps_to_max_start() {
        // 15 items, window 10 — max start is 5
        assert_eq!(compute_viewport(14, 0, 15, 10), 5);
    }

    #[test]
    fn viewport_empty_list() {
        assert_eq!(compute_viewport(0, 0, 0, 10), 0);
    }

    #[test]
    fn viewport_single_item() {
        assert_eq!(compute_viewport(0, 0, 1, 10), 0);
    }

    #[test]
    fn viewport_exactly_window_size() {
        // 10 items, window 10 — no scrolling needed
        assert_eq!(compute_viewport(0, 0, 10, 10), 0);
        assert_eq!(compute_viewport(9, 0, 10, 10), 0);
    }
}
