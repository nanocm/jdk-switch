use async_trait::async_trait;
use crate::downloader::traits::{JdkPackage, JdkSource, detect_arch, detect_os, get_file_type};
use crate::error::JdkError;
use crate::error::Result;
use reqwest::Client;
use reqwest::StatusCode;
use futures_util::stream::{self, StreamExt};
use serde::Deserialize;

pub struct AdoptiumSource {
    client: Client,
}

impl AdoptiumSource {
    pub fn new() -> Result<Self> {
        Ok(Self {
            client: Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .build()
                .map_err(|e| JdkError::NetworkError(e.to_string()))?,
        })
    }

    async fn available_releases(&self) -> Result<AvailableReleases> {
        self.client.get("https://api.adoptium.net/v3/info/available_releases")
            .send().await
            .map_err(|e| JdkError::NetworkError(e.to_string()))?
            .error_for_status()
            .map_err(|e| JdkError::NetworkError(e.to_string()))?
            .json().await
            .map_err(|e| JdkError::NetworkError(e.to_string()))
    }

    /// version + os + arch + lts to find jdk info
    async fn fetch_version_package(
        &self,
        version: u32,
        os: &str,
        arch: &str,
        lts_versions: &[u32],
    ) -> Result<JdkPackage> {
        let url = format!(
            "https://api.adoptium.net/v3/assets/latest/{}/hotspot?os={}&architecture={}&image_type=jdk",
            version, os, arch
        );
        let response = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| JdkError::NetworkError(e.to_string()))?;
        if matches!(response.status(), StatusCode::NOT_FOUND | StatusCode::NO_CONTENT) {
            return Err(JdkError::PackageNotFound(version.to_string()));
        }
        let mut response: Vec<AssetResponse> = response
            .error_for_status()
            .map_err(|e| JdkError::NetworkError(e.to_string()))?
            .json()
            .await
            .map_err(|e| JdkError::NetworkError(e.to_string()))?;

        let asset = response
            .pop()
            .ok_or_else(|| JdkError::PackageNotFound(version.to_string()))?;

        Ok(JdkPackage {
            version: asset.version.semver.clone(),
            runtime_version: asset.version.openjdk_version,
            major_version: asset.version.major,
            vendor: "temurin".to_string(),
            os: asset.binary.os.clone(),
            arch: asset.binary.architecture.clone(),
            download_url: asset.binary.package.link.clone(),
            mirror_urls: vec![format!(
                "https://mirrors.tuna.tsinghua.edu.cn/Adoptium/{version}/jdk/{arch}/{os}/{}",
                asset.binary.package.name
            )],
            size: asset.binary.package.size,
            file_type: get_file_type(os).to_string(),
            is_lts: lts_versions.contains(&version),
            checksum: Some(asset.binary.package.checksum),
            is_archived: false,
        })
    }
}

// ============adoptium api structs============
#[derive(Deserialize, Debug)]
pub struct AvailableReleases {
    available_releases: Vec<u32>,
    available_lts_releases: Vec<u32>,
}
#[derive(Deserialize, Debug)]
struct AssetResponse {
    binary: Binary,
    version: Version,
    // release_name: String,
}
#[derive(Deserialize, Debug)]
struct Version {
    major: u32,
    #[serde(default)]
    openjdk_version: Option<String>,
    // minor: u32,
    // security: u32,
    // build: u32,
    semver: String,
}
#[derive(Deserialize, Debug)]
struct Binary {
    architecture: String,
    os: String,
    package: Package,
}
#[derive(Deserialize, Debug)]
struct Package {
    name: String,
    link: String,
    size: u64,
    // name: String,
    checksum: String,
}
// ============adoptium api structs============

#[async_trait]
impl JdkSource for AdoptiumSource {
    fn name(&self) -> &str {
        "Eclipse Adoptium (Temurin)"
    }

    async fn fetch_version(&self) -> Result<Vec<JdkPackage>> {
        let releases = self.available_releases().await?;
        let os = detect_os();
        let arch = detect_arch();
        let mut packages = Vec::new();
        let results = stream::iter(releases.available_releases)
            .map(|version| self.fetch_version_package(version, &os, &arch, &releases.available_lts_releases))
            .buffer_unordered(4)
            .collect::<Vec<_>>().await;
        for result in results {
            match result {
                Ok(package) => packages.push(package),
                Err(JdkError::PackageNotFound(_)) => {},
                Err(error) => return Err(error),
            }
        }
        packages.sort_by_key(|package| package.major_version);
        Ok(packages)
    }

    async fn find_package(&self, major_version: u32) -> Result<JdkPackage> {
        let releases = self.available_releases().await?;
        if !releases.available_releases.contains(&major_version) {
            return Err(JdkError::PackageNotFound(major_version.to_string()));
        }
        self.fetch_version_package(major_version, &detect_os(), &detect_arch(),
            &releases.available_lts_releases).await
    }
}
