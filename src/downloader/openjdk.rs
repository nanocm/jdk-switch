use async_trait::async_trait;
use reqwest::{Client, header::CONTENT_LENGTH};
use roxmltree::{Document, ParsingOptions};
use std::collections::BTreeMap;

use crate::downloader::traits::{JdkPackage, JdkSource, detect_arch, detect_os, get_file_type};
use crate::error::{JdkError, Result};

const HOME: &str = "https://jdk.java.net/";
const ARCHIVE: &str = "https://jdk.java.net/archive/";

pub struct OpenJdkSource {
    client: Client,
}

struct Build {
    package: JdkPackage,
    checksum_url: String,
}

impl OpenJdkSource {
    pub fn new() -> Result<Self> {
        Ok(Self {
            client: Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .build()
                .map_err(|error| JdkError::NetworkError(error.to_string()))?,
        })
    }

    async fn page(&self, url: &str) -> Result<String> {
        self.client
            .get(url)
            .send()
            .await
            .map_err(|error| JdkError::NetworkError(error.to_string()))?
            .error_for_status()
            .map_err(|error| JdkError::NetworkError(error.to_string()))?
            .text()
            .await
            .map_err(|error| JdkError::NetworkError(error.to_string()))
    }

    async fn current_major(&self) -> Result<u32> {
        parse_current_major(&self.page(HOME).await?)
    }

    async fn current_builds(&self, major: u32) -> Result<Vec<Build>> {
        let url = format!("{HOME}{major}/");
        parse_builds(&self.page(&url).await?, false, &detect_os(), &detect_arch())
    }

    async fn archived_builds(&self) -> Result<Vec<Build>> {
        parse_builds(
            &self.page(ARCHIVE).await?,
            true,
            &detect_os(),
            &detect_arch(),
        )
    }
}

fn parse_current_major(html: &str) -> Result<u32> {
    let document = Document::parse_with_options(
        html,
        ParsingOptions {
            allow_dtd: true,
            ..Default::default()
        },
    )
    .map_err(|error| JdkError::DownloadError(format!("Cannot read OpenJDK homepage: {error}")))?;
    document
        .descendants()
        .filter(|node| node.is_element() && node.tag_name().name() == "a")
        .filter_map(|node| {
            let text = node.text()?.trim();
            let major = text.strip_prefix("JDK ")?.parse::<u32>().ok()?;
            (node.attribute("href") == Some(format!("/{major}/").as_str())).then_some(major)
        })
        .next()
        .ok_or_else(|| {
            JdkError::DownloadError("Current OpenJDK GA release was not found".to_string())
        })
}

fn platform_matches(label: &str, os: &str, arch: &str) -> bool {
    let label: String = label
        .chars()
        .filter(|char| char.is_ascii_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect();
    let os_matches = match os {
        "windows" => label.contains("windows"),
        "linux" => label.contains("linux"),
        "mac" => label.contains("mac"),
        _ => false,
    };
    os_matches
        && match arch {
            "aarch64" => label.contains("aarch64"),
            "x64" => {
                label.contains("x64") || (label.contains("64bit") && !label.contains("aarch64"))
            }
            _ => false,
        }
}

fn parse_builds(html: &str, archived: bool, os: &str, arch: &str) -> Result<Vec<Build>> {
    let document = Document::parse_with_options(
        html,
        ParsingOptions {
            allow_dtd: true,
            ..Default::default()
        },
    )
    .map_err(|error| JdkError::DownloadError(format!("Cannot read OpenJDK downloads: {error}")))?;
    let mut builds = Vec::new();

    for row in document
        .descendants()
        .filter(|node| node.is_element() && node.tag_name().name() == "tr")
    {
        let platform = row
            .children()
            .filter(|node| node.is_element() && node.tag_name().name() == "th")
            .flat_map(|node| node.descendants().filter_map(|child| child.text()))
            .collect::<Vec<_>>()
            .join(" ");
        if !platform_matches(&platform, os, arch) {
            continue;
        }
        let links = row
            .descendants()
            .filter(|node| node.is_element() && node.tag_name().name() == "a")
            .filter_map(|node| node.attribute("href"))
            .collect::<Vec<_>>();
        let Some(url) = links
            .iter()
            .find(|url| url.ends_with(".zip") || url.ends_with(".tar.gz"))
        else {
            continue;
        };
        let Some(checksum_url) = links.iter().find(|url| url.ends_with(".sha256")) else {
            continue;
        };
        let expected_checksum_url = format!("{url}.sha256");
        if !url.starts_with("https://download.java.net/")
            || *checksum_url != expected_checksum_url.as_str()
        {
            continue;
        }
        let filename = url.rsplit('/').next().unwrap_or("");
        let Some(version) = filename
            .strip_prefix("openjdk-")
            .and_then(|value| value.split('_').next())
        else {
            continue;
        };
        let Some(major) = version
            .split('.')
            .next()
            .and_then(|value| value.parse::<u32>().ok())
        else {
            continue;
        };
        let text = row
            .descendants()
            .filter_map(|node| node.text())
            .collect::<Vec<_>>()
            .join(" ");
        let size = text
            .split(|char: char| !char.is_ascii_digit())
            .find(|part| part.len() >= 7)
            .and_then(|part| part.parse::<u64>().ok())
            .or_else(|| {
                text.split_whitespace().find_map(|part| {
                    part.strip_suffix('M')?
                        .parse::<u64>()
                        .ok()?
                        .checked_mul(1024 * 1024)
                })
            })
            .unwrap_or(0);
        if size == 0 {
            continue;
        }
        builds.push(Build {
            package: JdkPackage {
                version: version.to_string(),
                runtime_version: Some(version.to_string()),
                major_version: major,
                vendor: "openjdk".to_string(),
                os: os.to_string(),
                arch: arch.to_string(),
                download_url: (*url).to_string(),
                mirror_urls: vec![format!(
                    "https://mirrors.huaweicloud.com/openjdk/{version}/{filename}"
                )],
                size,
                file_type: get_file_type(os).to_string(),
                is_lts: matches!(major, 8 | 11 | 17 | 21 | 25),
                checksum: None,
                is_archived: archived,
            },
            checksum_url: (*checksum_url).to_string(),
        });
    }
    Ok(builds)
}

#[async_trait]
impl JdkSource for OpenJdkSource {
    fn name(&self) -> &str {
        "OpenJDK (jdk.java.net)"
    }

    async fn fetch_version(&self) -> Result<Vec<JdkPackage>> {
        let current = self.current_major().await?;
        let (current_builds, archived_builds) =
            tokio::try_join!(self.current_builds(current), self.archived_builds())?;
        let mut packages = BTreeMap::<u32, JdkPackage>::new();
        // The official archive is sorted newest first within each major.
        for build in archived_builds {
            packages
                .entry(build.package.major_version)
                .or_insert(build.package);
        }
        if let Some(build) = current_builds
            .into_iter()
            .find(|build| build.package.major_version == current)
        {
            packages.insert(current, build.package);
        }
        Ok(packages.into_values().collect())
    }

    async fn find_package(&self, major_version: u32) -> Result<JdkPackage> {
        let current = self.current_major().await?;
        let builds = if major_version == current {
            self.current_builds(current).await?
        } else {
            self.archived_builds().await?
        };
        let mut build = builds
            .into_iter()
            .find(|build| build.package.major_version == major_version)
            .ok_or_else(|| JdkError::PackageNotFound(major_version.to_string()))?;
        let head = self
            .client
            .head(&build.package.download_url)
            .send()
            .await
            .map_err(|error| JdkError::NetworkError(error.to_string()))?
            .error_for_status()
            .map_err(|error| JdkError::NetworkError(error.to_string()))?;
        build.package.size = head
            .headers()
            .get(CONTENT_LENGTH)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.parse::<u64>().ok())
            .filter(|size| *size > 0)
            .ok_or_else(|| {
                JdkError::DownloadError("OpenJDK archive size is missing".to_string())
            })?;
        let checksum_text = self.page(&build.checksum_url).await?;
        let checksum = checksum_text
            .split_whitespace()
            .next()
            .filter(|value| value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit()))
            .ok_or_else(|| JdkError::DownloadError("Invalid OpenJDK SHA-256".to_string()))?;
        build.package.checksum = Some(checksum.to_string());
        Ok(build.package)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_current_ga_and_platform_specific_archive_builds() {
        let home = r#"<html><body><h1>Ready for use: <a href="/27/">JDK 27</a></h1></body></html>"#;
        assert_eq!(parse_current_major(home).unwrap(), 27);
        let archive = r#"<html><body><table><tr><th>21.0.2 (build 21.0.2+13)</th></tr>
          <tr><th>Windows</th><th>64-bit</th><td><a href="https://download.java.net/java/GA/openjdk-21.0.2_windows-x64_bin.zip">zip</a> 201328186 <a href="https://download.java.net/java/GA/openjdk-21.0.2_windows-x64_bin.zip.sha256">sha256</a></td></tr>
          <tr><th>macOS / x64</th><td><a href="https://download.java.net/java/GA/openjdk-21.0.2_macos-x64_bin.tar.gz">tar.gz</a> 200000000 <a href="https://download.java.net/java/GA/openjdk-21.0.2_macos-x64_bin.tar.gz.sha256">sha256</a></td></tr>
          <tr><th>macOS / AArch64</th><td><a href="https://download.java.net/java/GA/openjdk-21.0.2_macos-aarch64_bin.tar.gz">tar.gz</a> 197537405 <a href="https://download.java.net/java/GA/openjdk-21.0.2_macos-aarch64_bin.tar.gz.sha256">sha256</a></td></tr>
          </table></body></html>"#;
        for (os, arch, filename) in [
            ("windows", "x64", "openjdk-21.0.2_windows-x64_bin.zip"),
            ("mac", "x64", "openjdk-21.0.2_macos-x64_bin.tar.gz"),
            ("mac", "aarch64", "openjdk-21.0.2_macos-aarch64_bin.tar.gz"),
        ] {
            let builds = parse_builds(archive, true, os, arch).unwrap();
            assert_eq!(builds.len(), 1);
            assert!(builds[0].package.is_archived);
            assert_eq!(builds[0].package.version, "21.0.2");
            assert!(builds[0].package.download_url.ends_with(filename));
        }
    }
}
