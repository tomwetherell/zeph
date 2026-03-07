use std::io::{self, Write};

use crossterm::style::{Attribute, Print, ResetColor, SetAttribute, SetForegroundColor};

use super::{all_commands, CommandAction, CommandResult};
use crate::ui::style;

pub fn run() -> CommandResult {
    let mut out = io::stdout();
    let _ = crossterm::execute!(
        out,
        Print("\n"),
        SetForegroundColor(style::HEADING),
        SetAttribute(Attribute::Bold),
        Print("Available commands:\n"),
        SetAttribute(Attribute::Reset),
        ResetColor,
        Print("\n"),
    );

    for cmd in all_commands() {
        let _ = crossterm::execute!(
            out,
            Print("  "),
            SetForegroundColor(style::HEADING),
            Print(cmd.name),
            ResetColor,
        );
        let pad = 12usize.saturating_sub(cmd.name.len());
        let _ = write!(out, "{}", " ".repeat(pad));
        let _ = crossterm::execute!(
            out,
            SetForegroundColor(style::DIM),
            Print(cmd.description),
            ResetColor,
            Print("\n"),
        );
    }

    let _ = writeln!(out);
    CommandResult {
        action: CommandAction::Continue,
        subtitle: None,
    }
}
