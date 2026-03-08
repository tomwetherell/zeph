mod autocomplete;
mod input;

use std::io::{self, Write};

use crossterm::style::{Print, ResetColor, SetForegroundColor};

use crate::commands::{self, CommandAction, Ctx};
use crate::ui::style;

pub fn run(ctx: &Ctx) -> anyhow::Result<()> {
    let commands = commands::all_commands();
    loop {
        match input::read_input(&commands)? {
            input::Input::Command(name) => {
                let result = commands::execute(&name, ctx);
                if let Some(msg) = &result.subtitle {
                    let mut out = io::stdout();
                    let _ = crossterm::execute!(
                        out,
                        SetForegroundColor(style::DIM),
                        Print(format!("  ⎿  {msg}\n")),
                        ResetColor,
                    );
                    let _ = out.flush();
                }
                match result.action {
                    CommandAction::Quit => break,
                    CommandAction::Continue => {}
                }
            }
            input::Input::Quit => break,
            input::Input::Empty => continue,
        }
    }
    Ok(())
}
