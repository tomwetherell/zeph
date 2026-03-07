mod help;

pub struct Command {
    pub name: &'static str,
    pub description: &'static str,
    pub handler: fn() -> CommandResult,
}

pub enum CommandResult {
    Continue,
    Quit,
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
            handler: || CommandResult::Quit,
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
    CommandResult::Continue
}
