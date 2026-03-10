mod help;
mod summary;

use zeph::zarr::metadata::StoreMeta;
use zeph::zarr::store::StoreLocation;

pub struct Ctx {
    pub store: StoreLocation,
    pub meta: StoreMeta,
}

pub struct Command {
    pub name: &'static str,
    pub description: &'static str,
    pub aliases: &'static [&'static str],
    pub handler: fn(&Ctx) -> CommandResult,
}

pub enum CommandAction {
    Continue,
    Quit,
}

pub struct CommandResult {
    pub action: CommandAction,
    pub subtitle: Option<String>,
}

pub fn all_commands() -> Vec<Command> {
    vec![
        Command {
            name: "/summary",
            description: "Show store overview",
            aliases: &[],
            handler: summary::run,
        },
        Command {
            name: "/help",
            description: "Show available commands",
            aliases: &[],
            handler: help::run,
        },
        Command {
            name: "/exit",
            description: "Exit zeph",
            aliases: &["/quit"],
            handler: |_| CommandResult {
                action: CommandAction::Quit,
                subtitle: Some("Bye!".into()),
            },
        },
    ]
}

pub fn execute(name: &str, ctx: &Ctx) -> CommandResult {
    for cmd in all_commands() {
        if cmd.name == name {
            return (cmd.handler)(ctx);
        }
    }
    eprintln!("Unknown command: {name}");
    CommandResult {
        action: CommandAction::Continue,
        subtitle: None,
    }
}
