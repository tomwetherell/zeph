mod cli;
mod commands;
mod repl;
mod ui;

use clap::Parser;

fn main() -> anyhow::Result<()> {
    // Ensure terminal is restored on panic
    let original_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = crossterm::terminal::disable_raw_mode();
        original_hook(info);
    }));

    let _cli = cli::Cli::parse();

    ui::welcome::render()?;
    repl::run()?;

    println!("\nGoodbye!");
    Ok(())
}
