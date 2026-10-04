use crate::error::{JdkError, Result};
use crate::jdk::detector::JdkDetector;
use std::fs;
use std::path::{Path, PathBuf};

pub struct Extractor;

impl Extractor {
    pub fn new() -> Self {
        Self
    }

    /// extract file to target dir
    pub fn extract(&self, archive_path: &Path, target_dir: &Path) -> Result<PathBuf> {
        fs::create_dir_all(target_dir).map_err(|e| JdkError::IoError(e))?;
        let name = archive_path.file_name().and_then(|s| s.to_str())
            .ok_or_else(|| JdkError::ExtractionError("Unknown file type".to_string()))?;
        if name.ends_with(".zip") {
            self.extract_zip(archive_path, target_dir)
        } else if name.ends_with(".tar.gz") {
            self.extract_tar_gz(archive_path, target_dir)
        } else {
            Err(JdkError::ExtractionError(format!("Unsupported archive format: {name}")))
        }
    }

    fn extract_zip(&self, archive_path: &Path, target_dir: &Path) -> Result<PathBuf> {
        use std::fs::File;
        use std::io;
        use zip::ZipArchive;

        let file = File::open(archive_path).map_err(|e| JdkError::IoError(e))?;
        let mut archive =
            ZipArchive::new(file).map_err(|e| JdkError::ExtractionError(e.to_string()))?;

        for i in 0..archive.len() {
            let mut file = archive
                .by_index(i)
                .map_err(|e| JdkError::ExtractionError(e.to_string()))?;
            let outpath = match file.enclosed_name() {
                Some(path) => target_dir.join(path),
                None => continue,
            };
            if file.is_dir() {
                fs::create_dir_all(&outpath).map_err(|error| JdkError::ExtractionError(
                    format!("Cannot create {}: {error}", outpath.display())))?;
            } else {
                if let Some(parent) = outpath.parent() {
                    fs::create_dir_all(parent).map_err(|error| JdkError::ExtractionError(
                        format!("Cannot create {}: {error}", parent.display())))?;
                }
                let mut outfile = File::create(&outpath).map_err(|error| JdkError::ExtractionError(
                    format!("Cannot write {}: {error}", outpath.display())))?;
                io::copy(&mut file, &mut outfile).map_err(|error| JdkError::ExtractionError(
                    format!("Cannot extract {}: {error}", outpath.display())))?;

                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    if let Some(mode) = file.unix_mode() {
                        fs::set_permissions(&outpath, fs::Permissions::from_mode(mode)).ok();
                    }
                }
            }
        }
        self.find_jdk_root(target_dir)
    }

    fn extract_tar_gz(&self, archive_path: &Path, target_dir: &Path) -> Result<PathBuf> {
        use flate2::read::GzDecoder;
        use tar::Archive;
        use std::fs::File;

        let file = File::open(archive_path)
            .map_err(|e| JdkError::IoError(e))?;
        let gz = GzDecoder::new(file);
        let mut archive = Archive::new(gz);

        archive.unpack(target_dir)
            .map_err(|e| JdkError::ExtractionError(e.to_string()))?;

        self.find_jdk_root(target_dir)
    }

    pub fn find_jdk_root(&self, base_dir: &Path) -> Result<PathBuf> {
        use walkdir::WalkDir;
        for entry in WalkDir::new(base_dir).max_depth(3) {
            let entry = entry.map_err(|error| JdkError::ExtractionError(format!(
                "Cannot inspect JDK under {}: {error}", base_dir.display())))?;
            let path = entry.path();

            if JdkDetector::is_valid_jdk(path) {
                return Ok(path.to_path_buf());
            }

        }
        Err(JdkError::ExtractionError("JDK root directory not found in archive".to_string()))
    }


}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_tar_gz_archive() {
        let dir = tempfile::tempdir().unwrap();
        let archive_path = dir.path().join("jdk.tar.gz");
        let archive_file = fs::File::create(&archive_path).unwrap();
        let encoder = flate2::write::GzEncoder::new(archive_file, flate2::Compression::default());
        let mut archive = tar::Builder::new(encoder);
        let mut header = tar::Header::new_gnu();
        header.set_size(0);
        header.set_mode(0o755);
        header.set_cksum();
        let java = if cfg!(windows) { "jdk-17/bin/java.exe" } else { "jdk-17/bin/java" };
        archive.append_data(&mut header, java, &[][..]).unwrap();
        let javac = if cfg!(windows) { "jdk-17/bin/javac.exe" } else { "jdk-17/bin/javac" };
        archive.append_data(&mut header, javac, &[][..]).unwrap();
        let mut lib_header = tar::Header::new_gnu();
        lib_header.set_entry_type(tar::EntryType::Directory);
        lib_header.set_size(0);
        lib_header.set_mode(0o755);
        lib_header.set_cksum();
        archive.append_data(&mut lib_header, "jdk-17/lib/", &[][..]).unwrap();
        archive.finish().unwrap();
        archive.into_inner().unwrap().finish().unwrap();

        let root = Extractor::new().extract(&archive_path, &dir.path().join("out")).unwrap();
        assert_eq!(root, dir.path().join("out").join("jdk-17"));
    }
}


