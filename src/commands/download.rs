use crate::config::Config;
use crate::downloader::downloader::Downloader;
use crate::downloader::extractor::Extractor;
use crate::downloader::progress::ProgressDisplay;
use crate::downloader::traits::JdkPackage;
use crate::downloader::Vendor;
use crate::error::{JdkError, Result};
use crate::jdk::JdkManager;
use crate::jdk::detector::JdkDetector;
use colored::Colorize;
use std::path::Path;

pub async fn download_command(version: &str, vendor: Vendor) -> Result<()> {
    println!("  Version: {version}");
    println!("  Vendor: {}", vendor.as_str());
    println!("{}", format!("Searching for JDK {version}...").cyan());

    let source = vendor.source()?;
    let version_num: u32 = version.parse()
        .map_err(|_| JdkError::InvalidVersion(version.to_string()))?;
    let package = source.find_package(version_num).await?;

    println!("\n{}", "Found package:".green().bold());
    println!("  Version:     {}", package.version);
    println!("  Vendor:      {}", package.vendor);
    println!("  Size:        {} MB", package.size / 1024 / 1024);
    println!("  Platform:    {} ({})", package.os, package.arch);
    println!("  File type:   {}", package.file_type);
    if package.is_lts { println!("  Support:     {}", "LTS (Long Term Support)".green()); }
    if package.is_archived {
        println!("{}", "  Warning:     Archived OpenJDK build; it no longer receives security fixes.".yellow());
    }

    let extractor = Extractor::new();
    let install_base = Config::load()?.install_dir()?;
    std::fs::create_dir_all(&install_base)?;
    let release_name = release_name(&package);
    let install_dir = install_base.join(&release_name);

    let jdk_path = if install_dir.exists() {
        let path = extractor.find_jdk_root(&install_dir)?;
        verify_installed_jdk(&path, &package)?;
        println!("{}", format!("Using installed JDK: {}", path.display()).green());
        path
    } else {
        let filename = format!("{release_name}.{}", package.file_type);
        println!("\n{}", "Downloading...".cyan());
        let downloader = Downloader::new()?;
        let archive_path = downloader.download_file(
            &package.download_url,
            &package.mirror_urls,
            &filename,
            package.size,
            package.checksum.as_deref(),
            ProgressDisplay::simple_callback(),
        ).await?;
        println!("{}", "[OK] Download complete".green());

        println!("\n{}", "Extracting...".cyan());
        // A failed extraction leaves only a temporary directory. An existing
        // installation is never overwritten by files from another archive.
        let staging = tempfile::Builder::new().prefix(".jsh-staging-").tempdir_in(&install_base)
            .map_err(|error| JdkError::ExtractionError(format!(
                "Cannot create staging directory in {}: {error}", install_base.display())))?;
        let staged_root = extractor.extract(&archive_path, staging.path())?;
        verify_installed_jdk(&staged_root, &package)?;
        let relative_root = staged_root.strip_prefix(staging.path())
            .map_err(|e| JdkError::ExtractionError(e.to_string()))?;
        let installed_root = match move_staging(staging.path(), &install_dir).await {
            Ok(()) => install_dir.join(relative_root),
            Err(_) if install_dir.exists() => {
                // Another download command may have installed the same release.
                let existing = extractor.find_jdk_root(&install_dir)?;
                verify_installed_jdk(&existing, &package)?;
                existing
            }
            Err(error) => return Err(JdkError::ExtractionError(format!(
                "Cannot move {} to {}: {error}", staging.path().display(), install_dir.display()))),
        };
        println!("{}", format!("[OK] Extracted to: {}", installed_root.display()).green());
        installed_root
    };

    let mut manager = JdkManager::new()?;
    let id = manager.register_jdk_path(&jdk_path)?;
    println!("\n{} {}", "[SUCCESS]".green().bold(), "JDK installed and registered!".green());
    println!("  ID: {id}");
    println!("\n{}", "Next steps:".bold());
    println!("  1. List all JDKs:    {}", "jsh list".cyan());
    println!("  2. Activate this JDK: {}", format!("jsh use {id}").cyan());
    Ok(())
}

async fn move_staging(from: &Path, to: &Path) -> std::io::Result<()> {
    let mut delay = std::time::Duration::from_millis(150);
    for attempt in 0..10 {
        match std::fs::rename(from, to) {
            Err(error) if cfg!(windows)
                && error.kind() == std::io::ErrorKind::PermissionDenied
                && !to.exists() && attempt < 9 => {
                    // Virus scanners can briefly hold a newly extracted Java
                    // executable open on Windows. Keep the staged install
                    // intact while the file handle is released.
                    tokio::time::sleep(delay).await;
                    delay = (delay * 2).min(std::time::Duration::from_secs(2));
                }
            result => return result,
        }
    }
    unreachable!()
}

fn safe_component(value: &str) -> String {
    value.chars()
        .map(|c| if c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-') { c } else { '_' })
        .collect()
}

fn release_name(package: &JdkPackage) -> String {
    format!("jdk-{}-{}-{}-{}",
        safe_component(&package.vendor), safe_component(&package.version),
        safe_component(&package.os), safe_component(&package.arch))
}

fn verify_installed_jdk(path: &Path, package: &JdkPackage) -> Result<()> {
    if !JdkDetector::is_valid_jdk(path) {
        return Err(JdkError::InvalidPath(path.display().to_string()));
    }
    let actual = JdkDetector::get_jdk_info(path)
        .ok_or_else(|| JdkError::InvalidPath(path.display().to_string()))?;
    if actual.version != package.major_version.to_string() {
        return Err(JdkError::ExtractionError(format!(
            "Expected JDK {}, found JDK {} at {}",
            package.major_version, actual.version, path.display()
        )));
    }
    let expected = expected_java_version(package);
    if !actual.java_version.as_deref().is_some_and(|version| {
        java_version_matches(version, &expected, package)
    }) {
        return Err(JdkError::ExtractionError(format!(
            "Expected Java version {expected}, found {} at {}",
            actual.java_version.as_deref().unwrap_or("unknown"), path.display()
        )));
    }
    Ok(())
}

fn expected_java_version(package: &JdkPackage) -> String {
    let reported = package.runtime_version.as_deref().unwrap_or(&package.version);
    let version = reported.split(['+', '-']).next().unwrap_or(reported);
    if package.major_version == 8 && let Some(update) = version.strip_prefix("8.0.") {
        return format!("1.8.0_{update}");
    }
    version.to_string()
}

fn java_version_matches(actual: &str, expected: &str, package: &JdkPackage) -> bool {
    if actual == expected {
        return true;
    }
    if package.vendor != "corretto" || package.major_version < 9 {
        return false;
    }
    // Corretto's release name identifies the upstream feature, interim, and
    // security version, but does not consistently encode the optional fourth
    // Java version component across platforms.
    let numbers = |value: &str| {
        value.split('.').map(str::parse::<u32>)
            .collect::<std::result::Result<Vec<_>, _>>()
    };
    let (Ok(actual), Ok(expected)) = (numbers(actual), numbers(expected)) else {
        return false;
    };
    actual.len() <= 4 && (0..3).all(|index| {
        actual.get(index).copied().unwrap_or(0) == expected.get(index).copied().unwrap_or(0)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn temurin_semver_matches_java_version_output() {
        let mut package = JdkPackage {
            version: "8.0.504+1".to_string(), major_version: 8,
            runtime_version: Some("1.8.0_504-b01".to_string()),
            vendor: "temurin".to_string(), os: "windows".to_string(),
            arch: "x64".to_string(), download_url: String::new(),
            mirror_urls: Vec::new(),
            size: 0, file_type: "zip".to_string(), is_lts: true,
            checksum: None, is_archived: false,
        };
        assert_eq!(expected_java_version(&package), "1.8.0_504");
        package.version = "21.0.12+101.0.LTS".to_string();
        package.runtime_version = Some("21.0.12.1+1-LTS".to_string());
        package.major_version = 21;
        assert_eq!(expected_java_version(&package), "21.0.12.1");
    }

    #[test]
    fn corretto_runtime_accepts_a_fourth_java_version_component() {
        let package = JdkPackage {
            version: "21.0.12.12.1".to_string(),
            runtime_version: Some("21.0.12".to_string()),
            major_version: 21,
            vendor: "corretto".to_string(),
            os: "mac".to_string(),
            arch: "aarch64".to_string(),
            download_url: String::new(),
            mirror_urls: Vec::new(),
            size: 0,
            file_type: "tar.gz".to_string(),
            is_lts: true,
            checksum: None,
            is_archived: false,
        };
        assert!(java_version_matches("21.0.12.1", "21.0.12", &package));
        assert!(!java_version_matches("21.0.13", "21.0.12", &package));
        assert!(!java_version_matches("22.0.12.1", "21.0.12", &package));
    }
}
