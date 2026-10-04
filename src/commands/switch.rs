use crate::env::{EnvUpdater, get_env_updater};
#[cfg(not(target_os = "windows"))]
use crate::env::unix::UnixEnvUpdater;
use crate::error::{JdkError, Result};
use crate::jdk::JdkManager;
use colored::*;

pub fn use_command(version: &str) -> Result<()> {
    let mut manager = JdkManager::new()?;

    println!("{}", format!("Switching to JDK {}...", version).cyan());

    let (key, jdk) = manager.resolve_jdk(version)?;

    println!("\n{}", "Selected JDK:".bold());
    println!("  {} JDK {}", "Version:".bright_black(), jdk.version.green());
    println!("  {} {}", "ID:".bright_black(), key);
    println!("  {} {}", "Path:".bright_black(), jdk.path.display());

    // Update environment variables
    println!("\n{}", "Activating JDK...".cyan());
    let env_updater = get_env_updater();
    #[cfg(not(target_os = "windows"))]
    let rc_path = UnixEnvUpdater::shell_rc_path()?;
    let previous = manager.get_current_version().cloned();
    manager.set_current(key)?;
    if let Err(error) = env_updater.update_java_home(&jdk.path) {
        if let Err(rollback) = manager.set_current_option(previous) {
            return Err(JdkError::EnvError(format!(
                "{error}; restoring the previous JDK selection also failed: {rollback}"
            )));
        }
        return Err(error);
    }

    println!("\n{} {}", "[OK]".green().bold(), "Successfully switched to JDK".green());

    #[cfg(target_os = "windows")]
    {
        if env_updater.shell_uses_link()? {
            println!("  This terminal already uses the stable JDK path; java commands switch immediately.");
        } else {
            println!("\n{}", "One-time setup:".yellow().bold());
            println!("  Reopen this terminal after the stable JDK path is added to your JAVA_HOME and PATH.");
            println!("  If another Java appears earlier on PATH, move or remove that entry.");
            println!("  Future jsh use commands will then affect an open terminal immediately.");
        }
    }

    #[cfg(not(target_os = "windows"))]
    {
        if env_updater.shell_uses_link()? {
            println!("  This terminal already uses the stable JDK path; java commands switch immediately.");
        } else {
            println!("\n{}", "One-time setup:".yellow().bold());
            println!("  Run {} in this terminal.",
                format!("source {}", UnixEnvUpdater::shell_quote(&rc_path.to_string_lossy())).green());
            println!("  Future jsh use commands will then affect this open terminal immediately.");
        }
    }
    
    Ok(())
}
