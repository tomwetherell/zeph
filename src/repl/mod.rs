mod autocomplete;
mod input;
mod picker;

use std::io::{self, Write};

use crossterm::style::{Print, ResetColor, SetForegroundColor};

use crate::commands::{self, CommandAction, Ctx, Handler};
use crate::ui::style;

pub fn run(ctx: &Ctx) -> anyhow::Result<()> {
    let commands = commands::all_commands();
    loop {
        match input::read_input(&commands)? {
            input::Input::Command(name) => {
                let cmd = commands.iter().find(|c| {
                    c.name == name || c.aliases.contains(&name.as_str())
                });
                let result = match cmd {
                    Some(c) => match &c.handler {
                        Handler::Immediate(f) => f(ctx),
                        Handler::TargetSelect(f) => {
                            match picker::run(&ctx.meta.arrays)? {
                                Some(idx) => f(ctx, &ctx.meta.arrays[idx]),
                                None => continue,
                            }
                        }
                    },
                    None => {
                        let mut out = io::stdout();
                        let _ = crossterm::execute!(
                            out,
                            SetForegroundColor(style::DIM),
                            Print(format!("  ⎿  Unknown command: {name}\n")),
                            ResetColor,
                        );
                        continue;
                    }
                };
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
                    CommandAction::Quit => {
                        println!();
                        break;
                    }
                    CommandAction::Continue => {}
                }
            }
            input::Input::Quit => break,
            input::Input::Empty => continue,
        }
    }
    Ok(())
}
