use async_trait::async_trait;
use futures_util::stream::{self, StreamExt};
use reqwest::{Client, header::CONTENT_LENGTH};
use serde::Deserialize;
use std::collections::BTreeMap;

use crate::downloader::traits::{JdkPackage, JdkSource, detect_arch, detect_os, get_file_type};
use crate::error::{JdkError, Result};

const API: &str = "https://api.azul.com/metadata/v1/zulu/packages/";

pub struct ZuluSource {
    client: Client,
}

#[derive(Clone, Deserialize)]
struct ListedPackage {
    package_uuid: String,
    name: String,
    java_version: Vec<u32>,
    distro_version: Vec<u32>,
}

#[derive(Deserialize)]
struct PackageDetails {
    download_url: String,
    size: u64,
    sha256_hash: String,
    #[serde(default)]
    support_term: Option<String>,
}

impl ZuluSource {
    pub fn new() -> Result<Self> {
        Ok(Self {
            client: Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .build()
                .map_err(|error| JdkError::NetworkError(error.to_string()))?,
        })
    }

    async fn list(&self, major: Option<u32>) -> Result<Vec<ListedPackage>> {
        let os = detect_os();
        let arch = detect_arch();
        let api_os = if os == "mac" { "macos" } else { &os };
        let api_arch = if arch == "x64" { "x86_64" } else { &arch };
        let file_type = get_file_type(&os);
        let mut request = self.client.get(API).query(&[
            ("os", api_os),
            ("arch", api_arch),
            ("archive_type", file_type),
            ("java_package_type", "jdk"),
            ("release_status", "ga"),
            ("latest", "true"),
            ("page_size", "1000"),
        ]);
        if let Some(major) = major {
            request = request.query(&[("java_version", major)]);
        }
        let listings: Vec<ListedPackage> = request
            .send()
            .await
            .map_err(|error| JdkError::NetworkError(error.to_string()))?
            .error_for_status()
            .map_err(|error| JdkError::NetworkError(error.to_string()))?
            .json()
            .await
            .map_err(|error| JdkError::NetworkError(error.to_string()))?;
        let suffix = package_suffix(&os, &arch);
        Ok(listings
            .into_iter()
            .filter(|package| package.name.contains("-ca-jdk") && package.name.ends_with(&suffix))
            .collect())
    }

    async fn package(&self, listed: ListedPackage) -> Result<JdkPackage> {
        let url = format!("{API}{}", listed.package_uuid);
        let details: PackageDetails = self
            .client
            .get(url)
            .send()
            .await
            .map_err(|error| JdkError::NetworkError(error.to_string()))?
            .error_for_status()
            .map_err(|error| JdkError::NetworkError(error.to_string()))?
            .json()
            .await
            .map_err(|error| JdkError::NetworkError(error.to_string()))?;
        if details.size == 0
            || details.sha256_hash.len() != 64
            || !details
                .sha256_hash
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit())
            || !details.download_url.starts_with("https://cdn.azul.com/")
        {
            return Err(JdkError::DownloadError(
                "Invalid Azul Zulu package metadata".to_string(),
            ));
        }
        let major = *listed
            .java_version
            .first()
            .ok_or_else(|| JdkError::DownloadError("Zulu version is missing".to_string()))?;
        let java_version = listed
            .java_version
            .iter()
            .map(u32::to_string)
            .collect::<Vec<_>>()
            .join(".");
        let distro_version = listed
            .distro_version
            .iter()
            .map(u32::to_string)
            .collect::<Vec<_>>()
            .join(".");
        let runtime_version = runtime_version(&listed.java_version);
        let os = detect_os();
        Ok(JdkPackage {
            version: format!("{java_version}-zulu{distro_version}"),
            runtime_version,
            major_version: major,
            vendor: "zulu".to_string(),
            os: os.clone(),
            arch: detect_arch(),
            download_url: details.download_url,
            mirror_urls: Vec::new(),
            size: details.size,
            file_type: get_file_type(&os).to_string(),
            is_lts: details.support_term.as_deref() == Some("lts")
                || matches!(major, 8 | 11 | 17 | 21 | 25),
            checksum: Some(details.sha256_hash),
            is_archived: false,
        })
    }
}

fn package_suffix(os: &str, arch: &str) -> String {
    match os {
        "windows" => format!("-win_{arch}.zip"),
        "linux" => format!("-linux_{arch}.tar.gz"),
        "mac" => format!("-macosx_{arch}.tar.gz"),
        _ => String::new(),
    }
}

fn runtime_version(parts: &[u32]) -> Option<String> {
    let major = *parts.first()?;
    if major <= 8 {
        return parts.get(2).map(|update| format!("1.{major}.0_{update}"));
    }
    Some(
        parts
            .iter()
            .map(u32::to_string)
            .collect::<Vec<_>>()
            .join("."),
    )
}

fn latest_per_major(packages: Vec<ListedPackage>) -> Vec<ListedPackage> {
    let mut latest = BTreeMap::<u32, ListedPackage>::new();
    for package in packages {
        let Some(major) = package.java_version.first().copied() else {
            continue;
        };
        let replace = latest.get(&major).is_none_or(|current| {
            (&package.java_version, &package.distro_version)
                > (&current.java_version, &current.distro_version)
        });
        if replace {
            latest.insert(major, package);
        }
    }
    latest.into_values().collect()
}

#[async_trait]
impl JdkSource for ZuluSource {
    fn name(&self) -> &str {
        "Azul Zulu"
    }

    async fn fetch_version(&self) -> Result<Vec<JdkPackage>> {
        let listed = latest_per_major(self.list(None).await?);
        let results = stream::iter(listed)
            .map(|package| self.package(package))
            .buffer_unordered(6)
            .collect::<Vec<_>>()
            .await;
        let mut packages = results.into_iter().collect::<Result<Vec<_>>>()?;
        packages.sort_by_key(|package| package.major_version);
        Ok(packages)
    }

    async fn find_package(&self, major_version: u32) -> Result<JdkPackage> {
        let listed = latest_per_major(self.list(Some(major_version)).await?)
            .into_iter()
            .find(|package| package.java_version[0] == major_version)
            .ok_or_else(|| JdkError::PackageNotFound(major_version.to_string()))?;
        let mut package = self.package(listed).await?;
        // Azul's metadata size can lag behind the file on its CDN. The
        // SHA-256 is authoritative; read the current length from the file.
        let head = self
            .client
            .head(&package.download_url)
            .send()
            .await
            .map_err(|error| JdkError::NetworkError(error.to_string()))?
            .error_for_status()
            .map_err(|error| JdkError::NetworkError(error.to_string()))?;
        package.size = head
            .headers()
            .get(CONTENT_LENGTH)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.parse::<u64>().ok())
            .filter(|size| *size > 0)
            .ok_or_else(|| JdkError::DownloadError("Zulu archive size is missing".to_string()))?;
        Ok(package)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selects_standard_jdk_for_platform() {
        assert!("zulu21.52-ca-jdk21.0.12-linux_x64.tar.gz".contains("-ca-jdk"));
        assert!(
            "zulu21.52-ca-jdk21.0.12-linux_x64.tar.gz".ends_with(&package_suffix("linux", "x64"))
        );
        assert!(!"zulu21.52-ca-crac-jdk21.0.12-linux_x64.tar.gz".contains("-ca-jdk"));
        assert!(
            !"zulu21.52-ca-jdk21.0.12-linux_musl_x64.tar.gz"
                .ends_with(&package_suffix("linux", "x64"))
        );
        assert_eq!(package_suffix("mac", "aarch64"), "-macosx_aarch64.tar.gz");
    }

    #[test]
    fn maps_legacy_java_versions_to_runtime_output() {
        assert_eq!(runtime_version(&[6, 0, 119]).as_deref(), Some("1.6.0_119"));
        assert_eq!(runtime_version(&[7, 0, 352]).as_deref(), Some("1.7.0_352"));
        assert_eq!(runtime_version(&[8, 0, 504]).as_deref(), Some("1.8.0_504"));
        assert_eq!(
            runtime_version(&[21, 0, 12, 1]).as_deref(),
            Some("21.0.12.1")
        );
    }
}
