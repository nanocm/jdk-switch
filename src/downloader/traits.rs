use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use crate::error::{JdkError, Result};



#[async_trait]
pub trait JdkSource: Send + Sync {

    /// fetch source name
    fn name(&self) -> &str;

    /// fetch source available version
    async fn fetch_version(&self) -> Result<Vec<JdkPackage>>;

    /// find package by major version
    async fn find_package(&self, major_version: u32) -> Result<JdkPackage> {
        let packages = self.fetch_version().await?;
        if let Some(pkg) =  packages.iter().find(|p|p.major_version == major_version) {
            return Ok(pkg.clone());
        }
        Err(JdkError::JdkNotFound(major_version.to_string()))
    }

}

pub fn detect_os() -> String {
    #[cfg(target_os = "windows")]
    return "windows".to_string();

    #[cfg(target_os = "linux")]
    return "linux".to_string();

    #[cfg(target_os = "macos")]
    return "mac".to_string();
}

pub fn detect_arch() -> String {
    #[cfg(target_arch = "x86_64")]
    return "x64".to_string();

    #[cfg(target_arch = "aarch64")]
    return "aarch64".to_string();
}

pub fn get_file_type(os: &str) -> &'static str {
    match os {
        "windows" => "zip",
        "linux" | "mac" => "tar.gz",
        _ => "zip",
    }
}


/// JDK package info
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JdkPackage {
    /// version（eg: "17.0.9+9"）
    pub version: String,

    /// Version string reported by `java -version` before its build suffix.
    #[serde(default)]
    pub runtime_version: Option<String>,

    /// major version（eg: 17）
    pub major_version: u32,

    /// vendor name（eg: "temurin"）
    pub vendor: String,

    /// os（"windows", "linux", "mac"）
    pub os: String,

    /// arch（"x64", "aarch64"）
    pub arch: String,

    pub download_url: String,

    /// Optional mirrors of the same archive, checked against the same SHA-256.
    #[serde(default)]
    pub mirror_urls: Vec<String>,

    /// file size（bytes）
    pub size: u64,

    /// （"zip", "tar.gz"）
    pub file_type: String,

    pub is_lts: bool,

    /// SHA256 check sum
    pub checksum: Option<String>,

    /// The official OpenJDK archive is not updated with security fixes.
    #[serde(default)]
    pub is_archived: bool,
}
