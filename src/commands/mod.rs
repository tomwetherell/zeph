mod help;

pub struct Command {
    pub name: &'static str,
    pub description: &'static str,
    pub handler: fn() -> CommandResult,
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
            name: "/help",
            description: "Show available commands",
            handler: help::run,
        },
        Command {
            name: "/quit",
            description: "Exit zeph",
            handler: || CommandResult {
                action: CommandAction::Quit,
                subtitle: Some("Bye!".into()),
            },
        },
    ]
}

pub fn execute(name: &str) -> CommandResult {
    for cmd in all_commands() {
        if cmd.name == name {
            return (cmd.handler)();
        }
    }
    eprintln!("Unknown command: {name}");
    CommandResult {
        action: CommandAction::Continue,
        subtitle: None,
    }
}
