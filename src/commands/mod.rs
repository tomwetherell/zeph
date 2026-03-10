mod help;
mod info;
pub(crate) mod summary;

use zeph::zarr::metadata::{ArrayMeta, StoreMeta};
use zeph::zarr::store::StoreLocation;

pub struct Ctx {
    pub store: StoreLocation,
    pub meta: StoreMeta,
}

pub enum Handler {
    Immediate(fn(&Ctx) -> CommandResult),
    TargetSelect(fn(&Ctx, &ArrayMeta) -> CommandResult),
}

pub struct Command {
    pub name: &'static str,
    pub description: &'static str,
    pub aliases: &'static [&'static str],
    pub handler: Handler,
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
            handler: Handler::Immediate(summary::run),
        },
        Command {
            name: "/info",
            description: "Show variable details",
            aliases: &[],
            handler: Handler::TargetSelect(info::run),
        },
        Command {
            name: "/help",
            description: "Show available commands",
            aliases: &[],
            handler: Handler::Immediate(help::run),
        },
        Command {
            name: "/exit",
            description: "Exit zeph",
            aliases: &["/quit"],
            handler: Handler::Immediate(|_| CommandResult {
                action: CommandAction::Quit,
                subtitle: Some("Bye!".into()),
            }),
        },
    ]
}
