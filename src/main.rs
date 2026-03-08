mod cli;
mod commands;
mod repl;
mod ui;
mod zarr;

use clap::Parser;
use commands::Ctx;
use zarr::store::StoreLocation;

fn main() -> anyhow::Result<()> {
    // Ensure terminal is restored on panic
    let original_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = crossterm::terminal::disable_raw_mode();
        original_hook(info);
    }));

    let cli = cli::Cli::parse();

    let input = match cli.path {
        Some(p) => p,
        None => std::env::current_dir()?
            .to_string_lossy()
            .into_owned(),
    };

    let store = StoreLocation::parse(&input)?;
    let runtime = tokio::runtime::Runtime::new()?;

    ui::welcome::render(&store.display_path())?;

    let ctx = Ctx { store, runtime };
    repl::run(&ctx)?;

    Ok(())
}
