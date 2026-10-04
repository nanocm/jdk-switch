use crate::downloader::Vendor;
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(
    author,
    version,
    bin_name = "jsh",
    about = "Find, download, and switch JDKs",
    long_about = "Find installed JDKs, download verified packages, and switch the active Java from your terminal.",
    after_help = "Get started:\n  jsh list\n  jsh search 21\n  jsh download 21\n  jsh use 21\n  jsh current\n\nMore details: jsh help <COMMAND>\nConfiguration: jsh_config.json beside the jsh executable."
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    #[command(
        about = "List installed JDKs",
        long_about = "List registered JDKs and discover installs in managed directories, configured scan_dirs, and JAVA_HOME. Use --scan to search common system locations too.",
        after_help = "Examples:\n  jsh list\n  jsh list --scan\n  jsh list --prune\n\nAdd scan_dirs in jsh_config.json to search additional directories on every list."
    )]
    List {
        /// Also search common system locations (can be slow)
        #[arg(long)]
        scan: bool,
        /// Remove unavailable registrations; never delete JDK files
        #[arg(long)]
        prune: bool,
    },

    #[command(
        about = "Show the active JDK",
        long_about = "Show the JDK selected by JAVA_HOME (or by jsh when JAVA_HOME is unset) and report if java on PATH points somewhere else."
    )]
    Current,

    #[command(
        about = "Switch to an installed JDK",
        long_about = "Select an installed JDK by major version, full Java version, or ID from jsh list. If a version matches multiple installations, use its ID.",
        after_help = "Examples:\n  jsh use 21\n  jsh use 21.0.12\n\nFollow any one-time shell setup step printed by jsh. Later switches take effect in an open terminal when it uses the stable jsh-current path."
    )]
    Use {
        /// Major version, full Java version, or installation ID from jsh list
        #[arg(value_name = "VERSION_OR_ID")]
        version: String,
    },

    #[command(
        about = "Download and register a JDK",
        long_about = "Download the latest available build for a major JDK version, verify its size and SHA-256, install it, and register it for jsh list and jsh use.",
        after_help = "Examples:\n  jsh download 21\n  jsh download 21 --vendor corretto\n\nSet install_dir and download_dir in jsh_config.json to choose the install and archive cache directories."
    )]
    Download {
        /// Major JDK version, such as 8, 17, or 21
        #[arg(value_name = "MAJOR")]
        version: String,

        /// JDK distribution to download
        #[arg(long, value_enum, default_value_t = Vendor::Temurin)]
        vendor: Vendor,
    },

    #[command(
        about = "Search downloadable JDK releases",
        long_about = "Search the latest downloadable build of each major JDK version from one distribution. A numeric keyword matches a major version; other keywords match version or vendor text.",
        after_help = "Examples:\n  jsh search\n  jsh search 21\n  jsh search 21 --vendor zulu"
    )]
    Search {
        /// Optional major version or text to match
        keyword: Option<String>,

        /// JDK distribution to search
        #[arg(long, value_enum, default_value_t = Vendor::Temurin)]
        vendor: Vendor,
    },
}
