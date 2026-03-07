mod cli;
mod commands;
mod repl;
mod ui;

use std::path::PathBuf;

use clap::Parser;

fn main() -> anyhow::Result<()> {
    // Ensure terminal is restored on panic
    let original_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = crossterm::terminal::disable_raw_mode();
        original_hook(info);
    }));

    let cli = cli::Cli::parse();

    let store_path = match cli.path {
        Some(p) => PathBuf::from(p),
        None => std::env::current_dir()?,
    };
    let store_path_str = store_path.to_string_lossy();

    ui::welcome::render(&store_path_str)?;
    repl::run()?;

    println!("\nGoodbye!");
    Ok(())
}
