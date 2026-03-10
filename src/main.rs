mod cli;
mod commands;
mod repl;
mod ui;

use std::io::{self, Write};

use clap::Parser;
use crossterm::style::{Print, ResetColor, SetForegroundColor};

use commands::Ctx;
use zeph::zarr::metadata::{self, FetchError};
use zeph::zarr::store::StoreLocation;

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

    // For remote stores, show a brief status while connecting.
    let is_remote = matches!(store, StoreLocation::Cloud { .. });
    if is_remote {
        let mut out = io::stdout();
        let _ = crossterm::execute!(
            out,
            Print("\n"),
            SetForegroundColor(ui::style::DIM),
            Print(format!("  Connecting to {} ...", store.display_path())),
            ResetColor,
        );
        let _ = out.flush();
    }

    let meta = match metadata::fetch_store_meta(&store, &runtime) {
        Ok(meta) => {
            if is_remote {
                // Clear the "Connecting..." line
                let mut out = io::stdout();
                let _ = crossterm::execute!(
                    out,
                    Print("\r"),
                    crossterm::terminal::Clear(crossterm::terminal::ClearType::CurrentLine),
                );
            }
            meta
        }
        Err(e) => {
            let mut out = io::stderr();
            if is_remote {
                // Move to a new line after "Connecting..."
                let _ = crossterm::execute!(
                    out,
                    Print("\r"),
                    crossterm::terminal::Clear(crossterm::terminal::ClearType::CurrentLine),
                );
            }
            let _ = crossterm::execute!(
                out,
                Print("\n"),
                SetForegroundColor(ui::style::HEADING),
                Print("  Error: "),
                ResetColor,
            );
            match &e {
                FetchError::NotFound(msg)
                | FetchError::Unauthenticated(msg)
                | FetchError::PermissionDenied(msg)
                | FetchError::NoConsolidatedMetadata(msg) => {
                    for line in msg.lines() {
                        let _ = crossterm::execute!(out, Print(format!("  {line}\n")));
                    }
                }
                FetchError::Other(err) => {
                    let _ = crossterm::execute!(out, Print(format!("  {err:#}\n")));
                }
            }
            let _ = crossterm::execute!(out, Print("\n"));
            std::process::exit(1);
        }
    };

    ui::welcome::render(&store.display_path())?;

    let ctx = Ctx { store, meta };
    repl::run(&ctx)?;

    Ok(())
}
