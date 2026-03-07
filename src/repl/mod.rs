mod autocomplete;
mod input;

use crate::commands::{self, CommandResult};

pub fn run() -> anyhow::Result<()> {
    let commands = commands::all_commands();
    loop {
        match input::read_input(&commands)? {
            input::Input::Command(name) => match commands::execute(&name) {
                CommandResult::Quit => break,
                CommandResult::Continue => {}
            },
            input::Input::Quit => break,
            input::Input::Empty => continue,
        }
    }
    Ok(())
}
