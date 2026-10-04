use async_trait::async_trait;
use futures_util::stream::{self, StreamExt};
use reqwest::{Client, StatusCode, header::CONTENT_LENGTH};

use crate::downloader::traits::{JdkPackage, JdkSource, detect_arch, detect_os, get_file_type};
use crate::error::{JdkError, Result};

pub struct CorrettoSource {
    client: Client,
}

impl CorrettoSource {
    pub fn new() -> Result<Self> {
        Ok(Self {
            client: Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .build()
                .map_err(|error| JdkError::NetworkError(error.to_string()))?,
        })
    }

    async fn package(&self, major: u32) -> Result<JdkPackage> {
        let os = detect_os();
        let arch = detect_arch();
        let (platform, suffix) = match os.as_str() {
            "windows" => ("windows", "zip"),
            "linux" => ("linux", "tar.gz"),
            "mac" => ("macos", "tar.gz"),
            _ => return Err(JdkError::PackageNotFound(major.to_string())),
        };
        let filename = format!("amazon-corretto-{major}-{arch}-{platform}-jdk.{suffix}");
        let latest_url = format!("https://corretto.aws/downloads/latest/{filename}");
        let response = self
            .client
            .head(&latest_url)
            .send()
            .await
            .map_err(|error| JdkError::NetworkError(error.to_string()))?;
        if matches!(
            response.status(),
            StatusCode::NOT_FOUND | StatusCode::FORBIDDEN
        ) {
            return Err(JdkError::PackageNotFound(major.to_string()));
        }
        let response = response
            .error_for_status()
            .map_err(|error| JdkError::NetworkError(error.to_string()))?;
        let download_url = response.url().clone();
        if download_url.host_str() != Some("corretto.aws")
            || !download_url.path().starts_with("/downloads/resources/")
        {
            return Err(JdkError::DownloadError(
                "Unexpected Corretto download URL".to_string(),
            ));
        }
        let size = response
            .headers()
            .get(CONTENT_LENGTH)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.parse::<u64>().ok())
            .filter(|size| *size > 0)
            .ok_or_else(|| {
                JdkError::DownloadError("Corretto archive size is missing".to_string())
            })?;
        let resolved_name = download_url
            .path_segments()
            .and_then(|mut parts| parts.next_back())
            .ok_or_else(|| JdkError::DownloadError("Invalid Corretto archive URL".to_string()))?;
        let version = resolved_name
            .strip_prefix("amazon-corretto-")
            .and_then(|rest| rest.split('-').next())
            .filter(|value| value.starts_with(&format!("{major}.")))
            .ok_or_else(|| JdkError::DownloadError("Cannot read Corretto version".to_string()))?
            .to_string();
        let checksum_url = format!("https://corretto.aws/downloads/latest_sha256/{filename}");
        let checksum_text = self
            .client
            .get(checksum_url)
            .send()
            .await
            .map_err(|error| JdkError::NetworkError(error.to_string()))?
            .error_for_status()
            .map_err(|error| JdkError::NetworkError(error.to_string()))?
            .text()
            .await
            .map_err(|error| JdkError::NetworkError(error.to_string()))?;
        let checksum = checksum_text
            .split_whitespace()
            .next()
            .filter(|value| value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit()))
            .ok_or_else(|| JdkError::DownloadError("Invalid Corretto SHA-256".to_string()))?;

        Ok(JdkPackage {
            version: version.clone(),
            runtime_version: runtime_version(&version, major),
            major_version: major,
            vendor: "corretto".to_string(),
            os: os.clone(),
            arch,
            download_url: download_url.to_string(),
            mirror_urls: Vec::new(),
            size,
            file_type: get_file_type(&os).to_string(),
            is_lts: matches!(major, 8 | 11 | 17 | 21 | 25),
            checksum: Some(checksum.to_string()),
            is_archived: false,
        })
    }
}

fn runtime_version(version: &str, major: u32) -> Option<String> {
    let parts: Vec<_> = version.split('.').collect();
    if major == 8 {
        return parts.get(1).map(|update| format!("1.8.0_{update}"));
    }
    (parts.len() >= 3).then(|| parts[..3].join("."))
}

#[async_trait]
impl JdkSource for CorrettoSource {
    fn name(&self) -> &str {
        "Amazon Corretto"
    }

    async fn fetch_version(&self) -> Result<Vec<JdkPackage>> {
        // Corretto's latest endpoint has no version-list API. Probe the
        // maintained release lines; find_package also accepts newer majors.
        let majors = [8, 11, 17, 21, 25, 26, 27];
        let results = stream::iter(majors)
            .map(|major| self.package(major))
            .buffer_unordered(5)
            .collect::<Vec<_>>()
            .await;
        let mut packages = Vec::new();
        for result in results {
            match result {
                Ok(package) => packages.push(package),
                Err(JdkError::PackageNotFound(_)) => {}
                Err(error) => return Err(error),
            }
        }
        packages.sort_by_key(|package| package.major_version);
        Ok(packages)
    }

    async fn find_package(&self, major_version: u32) -> Result<JdkPackage> {
        self.package(major_version).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_corretto_release_to_java_version() {
        assert_eq!(
            runtime_version("8.504.04.1", 8).as_deref(),
            Some("1.8.0_504")
        );
        assert_eq!(
            runtime_version("21.0.12.12.1", 21).as_deref(),
            Some("21.0.12")
        );
    }
}
