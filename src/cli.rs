use clap::Parser;

#[derive(Parser, Debug)]
#[command(name = "zeph", version, about = "Inspect zarr data stores")]
pub struct Cli {
    /// Path to a zarr store (local or remote)
    pub path: Option<String>,
}
