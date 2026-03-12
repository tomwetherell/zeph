use std::io::{self, Write};

use crossterm::style::{Print, ResetColor, SetForegroundColor};

use super::{all_commands, CommandAction, CommandResult};

pub fn run(ctx: &super::Ctx) -> CommandResult {
    let mut out = io::stdout();
    let _ = crossterm::execute!(out, Print("\n"));

    for cmd in all_commands() {
        let _ = crossterm::execute!(
            out,
            Print("  "),
            SetForegroundColor(ctx.palette.heading),
            Print(cmd.name),
            ResetColor,
        );
        let pad = 12usize.saturating_sub(cmd.name.len());
        let _ = write!(out, "{}", " ".repeat(pad));
        let _ = crossterm::execute!(
            out,
            SetForegroundColor(ctx.palette.dim),
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
