use crate::config::Config;
use crate::error::{JdkError, Result};
use futures_util::StreamExt;
use reqwest::Client;
use sha2::{Digest, Sha256};
use std::io::Read;
use std::path::{Path, PathBuf};
use tokio::fs::File;
use tokio::io::AsyncWriteExt;

pub struct Downloader {
    client: Client,
    download_dir: PathBuf,
}

impl Downloader {
    pub fn new() -> Result<Self> {
        let config = Config::load()?;
        let download_dir = config.archive_dir()?;
        std::fs::create_dir_all(&download_dir)?;
        Ok(Self {
            client: Client::builder()
                .timeout(std::time::Duration::from_secs(600))
                .build()
                .map_err(|e| JdkError::DownloadError(e.to_string()))?,
            download_dir,
        })
    }

    pub async fn download_file<F>(
        &self,
        url: &str,
        mirror_urls: &[String],
        filename: &str,
        expected_size: u64,
        checksum: Option<&str>,
        on_progress: F,
    ) -> Result<PathBuf>
    where
        F: Fn(u64, u64) + Send + Sync + 'static,
    {
        if Path::new(filename).file_name().and_then(|s| s.to_str()) != Some(filename) {
            return Err(JdkError::DownloadError("Invalid archive filename".to_string()));
        }
        let target_path = self.download_dir.join(filename);
        if Self::verify_file(&target_path, expected_size, checksum)? {
            println!("Using verified cached file: {}", target_path.display());
            return Ok(target_path);
        }

        let mut failures = Vec::new();
        for source_url in std::iter::once(url).chain(mirror_urls.iter().map(String::as_str)) {
            if source_url != url { println!("Trying mirror: {source_url}"); }
            match self.download_from(source_url, &target_path, expected_size, checksum, &on_progress).await {
                Ok(path) => return Ok(path),
                Err(error) => failures.push(format!("{source_url}: {error}")),
            }
        }
        Err(JdkError::DownloadError(format!(
            "All download sources failed:\n  {}", failures.join("\n  ")
        )))
    }

    async fn download_from<F>(
        &self,
        url: &str,
        target_path: &Path,
        expected_size: u64,
        checksum: Option<&str>,
        on_progress: &F,
    ) -> Result<PathBuf>
    where
        F: Fn(u64, u64) + Send + Sync,
    {
        let response = self.client.get(url).send().await
            .map_err(|e| JdkError::NetworkError(e.to_string()))?
            .error_for_status()
            .map_err(|e| JdkError::NetworkError(e.to_string()))?;
        // Mirrors can contain a different build under the same filename. Do
        // not transfer the whole archive when its length already disagrees
        // with the package metadata from the authoritative source.
        if let Some(size) = response.content_length() && size != expected_size {
            return Err(JdkError::DownloadError(format!(
                "Archive size mismatch at {url}: expected {expected_size} bytes, found {size} bytes"
            )));
        }
        let part_path = tempfile::Builder::new().prefix(".jsh-download-")
            .suffix(".part").tempfile_in(&self.download_dir)?.into_temp_path();
        let mut file = File::create(&part_path).await?;
        let mut downloaded = 0_u64;
        let mut hasher = Sha256::new();
        let mut stream = response.bytes_stream();

        while let Some(chunk_result) = stream.next().await {
            let chunk = match chunk_result {
                Ok(chunk) => chunk,
                Err(e) => {
                    drop(file);
                    return Err(JdkError::NetworkError(e.to_string()));
                }
            };
            let next_size = downloaded.checked_add(chunk.len() as u64)
                .ok_or_else(|| JdkError::DownloadError(format!("Archive is too large: {url}")))?;
            if next_size > expected_size {
                return Err(JdkError::DownloadError(format!(
                    "Archive is larger than expected at {url}: expected {expected_size} bytes"
                )));
            }
            if let Err(e) = file.write_all(&chunk).await {
                drop(file);
                return Err(JdkError::IoError(e));
            }
            hasher.update(&chunk);
            downloaded = next_size;
            on_progress(downloaded, expected_size);
        }
        file.flush().await?;
        drop(file);

        let actual_checksum = format!("{:x}", hasher.finalize());
        if downloaded != expected_size || checksum.is_some_and(|value| !actual_checksum.eq_ignore_ascii_case(value)) {
            return Err(JdkError::DownloadError(format!(
                "Downloaded archive failed size or SHA-256 verification: {}", url
            )));
        }
        if Self::verify_file(&target_path, expected_size, checksum)? {
            return Ok(target_path.to_path_buf());
        }
        if let Err(error) = part_path.persist(&target_path) {
            if Self::verify_file(&target_path, expected_size, checksum)? {
                return Ok(target_path.to_path_buf());
            }
            return Err(JdkError::DownloadError(format!("Cannot save verified archive: {error}")));
        }
        Ok(target_path.to_path_buf())
    }

    fn verify_file(path: &Path, expected_size: u64, checksum: Option<&str>) -> Result<bool> {
        if !path.is_file() { return Ok(false); }
        if std::fs::metadata(path)?.len() != expected_size { return Ok(false); }
        let Some(checksum) = checksum else { return Ok(true); };
        let mut file = std::fs::File::open(path)?;
        let mut buffer = [0_u8; 64 * 1024];
        let mut hasher = Sha256::new();
        loop {
            let count = file.read(&mut buffer)?;
            if count == 0 { break; }
            hasher.update(&buffer[..count]);
        }
        Ok(format!("{:x}", hasher.finalize()).eq_ignore_ascii_case(checksum))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cache_requires_matching_size_and_sha256() {
        let dir = tempfile::tempdir().unwrap();
        let archive = dir.path().join("jdk.zip");
        std::fs::write(&archive, b"abc").unwrap();
        let checksum = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
        assert!(Downloader::verify_file(&archive, 3, Some(checksum)).unwrap());
        assert!(!Downloader::verify_file(&archive, 4, Some(checksum)).unwrap());
        assert!(!Downloader::verify_file(&archive, 3, Some(&"0".repeat(64))).unwrap());
    }

    #[tokio::test]
    async fn falls_back_to_a_mirror_and_verifies_the_archive() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let dir = tempfile::tempdir().unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let server = tokio::spawn(async move {
            for _ in 0..2 {
                let (mut stream, _) = listener.accept().await.unwrap();
                let mut request = [0_u8; 1024];
                let count = stream.read(&mut request).await.unwrap();
                let response = if String::from_utf8_lossy(&request[..count]).contains("/primary") {
                    "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                } else {
                    "HTTP/1.1 200 OK\r\nContent-Length: 3\r\nConnection: close\r\n\r\nabc"
                };
                stream.write_all(response.as_bytes()).await.unwrap();
            }
        });
        let downloader = Downloader {
            client: Client::new(),
            download_dir: dir.path().to_path_buf(),
        };
        let checksum = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
        let path = downloader.download_file(
            &format!("{base}/primary"), &[format!("{base}/mirror")],
            "jdk.zip", 3, Some(checksum), |_, _| {},
        ).await.unwrap();
        server.await.unwrap();
        assert_eq!(std::fs::read(path).unwrap(), b"abc");
    }

    #[tokio::test]
    async fn rejects_a_mirror_with_the_wrong_archive_size_before_reading_it() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let dir = tempfile::tempdir().unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let server = tokio::spawn(async move {
            for _ in 0..2 {
                let (mut stream, _) = listener.accept().await.unwrap();
                let mut request = [0_u8; 1024];
                let count = stream.read(&mut request).await.unwrap();
                let response = if String::from_utf8_lossy(&request[..count]).contains("/primary") {
                    "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                } else {
                    "HTTP/1.1 200 OK\r\nContent-Length: 4\r\nConnection: close\r\n\r\n"
                };
                stream.write_all(response.as_bytes()).await.unwrap();
            }
        });
        let downloader = Downloader {
            client: Client::new(),
            download_dir: dir.path().to_path_buf(),
        };
        let error = downloader.download_file(
            &format!("{base}/primary"), &[format!("{base}/mirror")],
            "jdk.zip", 3, None, |_, _| {},
        ).await.unwrap_err();
        server.await.unwrap();
        assert!(error.to_string().contains("Archive size mismatch"));
        assert!(!dir.path().join("jdk.zip").exists());
    }
}
