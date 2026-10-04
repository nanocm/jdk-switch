use clap::{Parser, Subcommand};
use crate::downloader::Vendor;

#[derive(Parser)]
#[command(author, version, about = "A tool to manage and switch between JDK installations", long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// List registered JDKs, managed installs, configured scan_dirs, and JAVA_HOME
    List {
        /// Also scan common system locations (can be slow)
        #[arg(long)]
        scan: bool,
        /// Remove registered JDKs whose paths are no longer valid
        #[arg(long)]
        prune: bool,
    },

    /// Show current active JDK
    Current,

    /// Switch to a specific JDK version
    Use {
        /// Major version, full version, or installation ID from `jsh list`
        version: String,
    },
    
    /// Download a specific JDK version
    Download {
        /// Version to download (e.g., 17, 21)
        version: String,

        /// JDK vendor: temurin, corretto, zulu, or openjdk
        #[arg(long, value_enum, default_value_t = Vendor::Temurin)]
        vendor: Vendor,
    },
    
    /// Search available JDK versions for download
    Search {
        /// Optional search keyword
        keyword: Option<String>,

        /// Search one vendor (default: temurin)
        #[arg(long, value_enum, default_value_t = Vendor::Temurin)]
        vendor: Vendor,
    },
}
