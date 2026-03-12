mod cli;
mod commands;
mod repl;
mod ui;

use std::io;

use clap::Parser;
use crossterm::style::{Print, ResetColor, SetForegroundColor};

use commands::Ctx;
use ui::spinner::Spinner;
use ui::style::{self, Palette};
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

    let palette = Palette::new(style::detect_color_support(), style::detect_theme());

    let store = StoreLocation::parse(&input)?;
    let runtime = tokio::runtime::Runtime::new()?;

    // For remote stores, show an animated spinner while connecting.
    let is_remote = matches!(store, StoreLocation::Cloud { .. });
    let spinner = if is_remote {
        Some(Spinner::start(
            "Connecting...",
            Some(&store.display_path()),
            &palette,
        ))
    } else {
        None
    };

    let meta = match metadata::fetch_store_meta(&store, &runtime) {
        Ok(meta) => {
            if let Some(sp) = spinner {
                sp.stop_with_message(&["Fetched .zmetadata"], &palette);
            }
            meta
        }
        Err(e) => {
            if let Some(sp) = spinner {
                sp.stop_with_message(&["Error fetching .zmetadata"], &palette);
            }
            let mut out = io::stderr();
            let _ = crossterm::execute!(
                out,
                Print("\n"),
                SetForegroundColor(palette.heading),
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

    ui::welcome::render(&store.display_path(), &palette)?;

    let ctx = Ctx { store, meta, palette };
    repl::run(&ctx)?;

    Ok(())
}
