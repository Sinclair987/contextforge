use std::{ffi::OsString, path::PathBuf};

use clap::{Parser, Subcommand};

use crate::{
    config::{write_default_config, DEFAULT_CONFIG_FILE},
    Result,
};

#[derive(Debug, Parser)]
#[command(name = "contextforge")]
#[command(about = "Compile local project context into auditable AI-ready bundles")]
pub struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    /// Generate a sample contextforge.toml configuration file.
    Init,
}

pub fn run<I, T>(args: I) -> Result<()>
where
    I: IntoIterator<Item = T>,
    T: Into<OsString> + Clone,
{
    let cli = Cli::parse_from(args);

    match cli.command {
        Commands::Init => init_current_directory(),
    }
}

fn init_current_directory() -> Result<()> {
    let path = PathBuf::from(DEFAULT_CONFIG_FILE);
    write_default_config(&path)?;
    println!("Created {}", path.display());
    Ok(())
}
