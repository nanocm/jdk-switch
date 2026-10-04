use crate::downloader::traits::JdkPackage;
use crate::downloader::Vendor;
use crate::error::Result;
use colored::Colorize;
use std::collections::HashMap;

pub async fn search_command(keyword: Option<String>, vendor: Vendor) -> Result<()> {
    println!("{}", "Searching for available JDK versions...".cyan());

    let source = vendor.source()?;
    let mut packages = source.fetch_version().await?;

    println!(
        "{}",
        format!("Found {} packages from {}", packages.len(), source.name()).bright_black()
    );

    if let Some(keyword) = keyword {
        packages.retain(|p| matches_keyword(p, &keyword));
        println!(
            "{}",
            format!(
                "Filtered to {} packages matching '{}'",
                packages.len(),
                keyword
            )
            .bright_black()
        );
        if packages.is_empty() {
            println!("\n{}", "No matching JDK versions found.".yellow());
            println!("Try searching without a keyword or with a different term.");
            return Ok(());
        }
    }

    let mut grouped: HashMap<u32, Vec<_>> = HashMap::new();
    for pkg in packages {
        grouped
            .entry(pkg.major_version)
            .or_insert_with(Vec::new)
            .push(pkg);
    }
    println!("\n{}", "Available JDK versions:".bold());
    println!("{}", "=".repeat(80).bright_black());

    let mut versions: Vec<_> = grouped.keys().collect();
    versions.sort_by(|a, b| b.cmp(a));

    for &version in &versions  {
        let pkgs = &grouped[version];
        let is_lts = pkgs[0].is_lts;
        // version number title
        let version_text = format!("JDK {}", version);
        let lts_tag = if is_lts {
            "(LTS)".green().bold()
        } else {
            "".normal()
        };
        println!("\n  {}{}", version_text.bold().cyan(), lts_tag);

        for pkg in pkgs {
            let size_mb = pkg.size / 1024 / 1024;
            let archive_tag = if pkg.is_archived { " [archived]" } else { "" };
            println!("    └─ {:8} {} {:>4}{}",
                     pkg.vendor.bright_black(),
                     pkg.version.white(),
                     format!("[{} MB]", size_mb).bright_black(),
                     archive_tag.yellow()
            );
        }
    }

    println!("\n{}", "-".repeat(80).bright_black());
    println!("Total: {} version(s)", versions.len());
    println!("\nUse: {} to download and install",
             format!("jsh download <version> --vendor {}", vendor.as_str()).green());
    if vendor == Vendor::Openjdk && versions.iter().any(|version| grouped[version][0].is_archived) {
        println!("{}", "Archived OpenJDK builds lack current security updates; use a maintained vendor for older releases.".yellow());
    }

    Ok(())
}

fn matches_keyword(package: &JdkPackage, keyword: &str) -> bool {
    if let Ok(major) = keyword.parse::<u32>() {
        return package.major_version == major;
    }
    let keyword = keyword.to_lowercase();
    package.version.to_lowercase().contains(&keyword)
        || package.vendor.to_lowercase().contains(&keyword)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn package(major_version: u32) -> JdkPackage {
        JdkPackage {
            version: format!("{major_version}.0.1+1"),
            runtime_version: None,
            major_version,
            vendor: "temurin".to_string(),
            os: "windows".to_string(),
            arch: "x64".to_string(),
            download_url: String::new(),
            mirror_urls: Vec::new(),
            size: 0,
            file_type: "zip".to_string(),
            is_lts: false,
            checksum: None,
            is_archived: false,
        }
    }

    #[test]
    fn numeric_search_matches_only_the_major_version() {
        assert!(matches_keyword(&package(8), "8"));
        assert!(!matches_keyword(&package(18), "8"));
        assert!(matches_keyword(&package(18), "temurin"));
    }
}
