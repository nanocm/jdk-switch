use crate::config::JdkInfo;
use crate::error::Result;
use std::path::{Path, PathBuf};
use std::process::Command;
use walkdir::WalkDir;

pub struct JdkDetector;

impl JdkDetector {
    /// Scan common system locations. On Windows these include drive roots,
    /// so callers should make this an explicit operation.
    pub fn detect_system_installations() -> Result<Vec<JdkInfo>> {
        let mut jdks = Vec::new();

        // Check common installation directories
        let search_paths = Self::get_search_paths();

        for search_path in search_paths {
            if let Ok(found) = Self::scan_directory(&search_path) {
                jdks.extend(found);
            }
        }
        
        // Deduplicate by path
        jdks.sort_by(|a, b| a.path.cmp(&b.path));
        jdks.dedup_by(|a, b| a.path == b.path);

        Ok(jdks)
    }

    pub fn detect_java_home() -> Option<JdkInfo> {
        if let Ok(java_home) = std::env::var("JAVA_HOME") {
            let path = PathBuf::from(java_home);
            if Self::is_valid_jdk(&path) {
                return Self::get_jdk_info(&path);
            }
        }
        None
    }
    
    /// Get common JDK installation paths based on OS
    fn get_search_paths() -> Vec<PathBuf> {
        let mut paths = Vec::new();

        #[cfg(target_os = "windows")]
        {
            paths.push(PathBuf::from("C:\\"));
            paths.push(PathBuf::from("D:\\"));
            paths.push(PathBuf::from("E:\\"));
            paths.push(PathBuf::from("F:\\"));
            paths.push(PathBuf::from("G:\\"));
        }
        
        #[cfg(target_os = "macos")]
        {
            paths.push(PathBuf::from("/Library/Java/JavaVirtualMachines"));
            paths.push(PathBuf::from("/System/Library/Java/JavaVirtualMachines"));
        }
        
        #[cfg(target_os = "linux")]
        {
            paths.push(PathBuf::from("/usr/lib/jvm"));
            paths.push(PathBuf::from("/usr/java"));
            paths.push(PathBuf::from("/opt/java"));
        }
        
        paths
    }
    
    /// Scan a directory for JDK installations
    pub fn scan_directory(path: &Path) -> Result<Vec<JdkInfo>> {
        let mut jdks = Vec::new();

        if !path.exists() {
            return Ok(jdks);
        }
        
        for entry in WalkDir::new(path)
            .max_depth(5)
            .follow_links(false)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            let path = entry.path();
            if Self::is_valid_jdk(path) {
                if let Some(info) = Self::get_jdk_info(path) {
                    jdks.push(info);
                }
            }
        }
        
        Ok(jdks)
    }

    /// Check if a directory is a valid JDK installation
    pub fn is_valid_jdk(path: &Path) -> bool {
        if !path.is_dir() {
            return false;
        }
        
        // A bundled JRE also has java and lib, but cannot compile Java code.
        let java_exe = Self::get_java_executable(path);
        let javac_exe = Self::get_javac_executable(path);
        if !java_exe.is_file() || !javac_exe.is_file() {
            return false;
        }
        
        // Check for required directories
        let lib_dir = path.join("lib");
        lib_dir.is_dir()
    }

    /// Get the java executable path for a JDK
    fn get_java_executable(jdk_path: &Path) -> PathBuf {
        #[cfg(target_os = "windows")]
        {
            jdk_path.join("bin").join("java.exe")
        }
        
        #[cfg(not(target_os = "windows"))]
        {
            jdk_path.join("bin").join("java")
        }
    }

    fn get_javac_executable(jdk_path: &Path) -> PathBuf {
        #[cfg(target_os = "windows")]
        { jdk_path.join("bin").join("javac.exe") }
        #[cfg(not(target_os = "windows"))]
        { jdk_path.join("bin").join("javac") }
    }
    
    /// Get JDK information by executing java -version
    pub fn get_jdk_info(path: &Path) -> Option<JdkInfo> {
        let java_exe = Self::get_java_executable(path);

        if !java_exe.exists() {
            return None;
        }
        
        let output = Command::new(&java_exe)
            .arg("-version")
            .output()
            .ok()?;

        if !output.status.success() {
            return None;
        }
        let output_text = if output.stderr.is_empty() { &output.stdout } else { &output.stderr };
        let (version, vendor, java_version) = Self::parse_version_output(&String::from_utf8_lossy(output_text));
        if version == "unknown" { return None; }

        Some(JdkInfo {
            path: Self::installation_path(path)?,
            version,
            vendor,
            java_version,
        })
    }

    fn installation_path(path: &Path) -> Option<PathBuf> {
        #[cfg(target_os = "windows")]
        {
            // junction::exists can error on an ordinary directory because it
            // queries reparse-point data. A regular JDK path is still valid.
            if let Ok(true) = junction::exists(path) {
                return junction::get_target(path).ok();
            }
        }
        #[cfg(unix)]
        {
            if path.symlink_metadata().ok()?.file_type().is_symlink() {
                return path.canonicalize().ok();
            }
        }
        Some(path.to_path_buf())
    }
    
    /// Parse java -version output
    fn parse_version_output(output: &str) -> (String, Option<String>, Option<String>) {
        let mut version = String::from("unknown");
        let mut vendor = None;
        let mut java_version = None;

        for line in output.lines() {
            // Parse version line: java version "1.8.0_291" or openjdk version "17.0.2"
            if line.contains("version") {
                if let Some(start) = line.find('"') {
                    if let Some(end) = line[start + 1..].find('"') {
                        let full_version = &line[start + 1..start + 1 + end];
                        java_version = Some(full_version.to_string());

                        // Extract major version
                        if full_version.starts_with("1.") {
                            // Old format: 1.8.0_291 -> 8
                            if let Some(major) = full_version.split('.').nth(1) {
                                version = major.to_string();
                            }
                        } else {
                            // New format: 17.0.2 -> 17
                            if let Some(major) = full_version.split('.').next() {
                                version = major.to_string();
                            }
                        }
                    }
                }
            }
            
        }

        // Prefer the specific distribution over generic OpenJDK text, which
        // appears in the version output of most vendors.
        if output.contains("Temurin") || output.contains("Eclipse") {
            vendor = Some("Eclipse Temurin".to_string());
        } else if output.contains("Corretto") {
            vendor = Some("Amazon Corretto".to_string());
        } else if output.contains("Zulu") {
            vendor = Some("Azul Zulu".to_string());
        } else if output.contains("Microsoft") {
            vendor = Some("Microsoft".to_string());
        } else if output.contains("Oracle") || output.contains("Java(TM)") {
            vendor = Some("Oracle".to_string());
        } else if output.contains("OpenJDK") || output.contains("openjdk") {
            vendor = Some("OpenJDK".to_string());
        }
        
        (version, vendor, java_version)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_a_jre_without_javac() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        std::fs::create_dir(root.join("bin")).unwrap();
        std::fs::create_dir(root.join("lib")).unwrap();
        let java = if cfg!(windows) { "java.exe" } else { "java" };
        let javac = if cfg!(windows) { "javac.exe" } else { "javac" };
        std::fs::write(root.join("bin").join(java), []).unwrap();
        assert!(!JdkDetector::is_valid_jdk(root));
        std::fs::write(root.join("bin").join(javac), []).unwrap();
        assert!(JdkDetector::is_valid_jdk(root));
    }

    #[test]
    fn test_parse_version_output() {
        let output1 = r#"java version "1.8.0_291"
Java(TM) SE Runtime Environment (build 1.8.0_291-b10)
Java HotSpot(TM) 64-Bit Server VM (build 25.291-b10, mixed mode)"#;
        
        let (version, _, _) = JdkDetector::parse_version_output(output1);
        assert_eq!(version, "8");

        let output2 = r#"openjdk version "17.0.2" 2022-01-18
OpenJDK Runtime Environment Temurin-17.0.2+8 (build 17.0.2+8)
OpenJDK 64-Bit Server VM Temurin-17.0.2+8 (build 17.0.2+8, mixed mode)"#;
        
        let (version, vendor, _) = JdkDetector::parse_version_output(output2);
        assert_eq!(version, "17");
        assert_eq!(vendor.as_deref(), Some("Eclipse Temurin"));

        let corretto = "openjdk version \"1.8.0_504\"\nOpenJDK Runtime Environment Corretto-8.504.04.1 (build 1.8.0_504-b04)";
        let (version, vendor, java_version) = JdkDetector::parse_version_output(corretto);
        assert_eq!(version, "8");
        assert_eq!(vendor.as_deref(), Some("Amazon Corretto"));
        assert_eq!(java_version.as_deref(), Some("1.8.0_504"));
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn ordinary_jdk_directory_keeps_its_path() {
        let temp = tempfile::tempdir().unwrap();
        assert_eq!(JdkDetector::installation_path(temp.path()), Some(temp.path().to_path_buf()));
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn jsh_junction_registers_its_real_target() {
        let temp = tempfile::tempdir().unwrap();
        let target = temp.path().join("jdk");
        std::fs::create_dir(&target).unwrap();
        let link = temp.path().join("jsh-current");
        junction::create(&target, &link).unwrap();
        assert!(crate::config::same_jdk_path(
            &JdkDetector::installation_path(&link).unwrap(), &target
        ));
    }

    #[cfg(unix)]
    #[test]
    fn jsh_symlink_registers_its_real_target() {
        let temp = tempfile::tempdir().unwrap();
        let target = temp.path().join("jdk");
        std::fs::create_dir(&target).unwrap();
        let link = temp.path().join("jsh-current");
        std::os::unix::fs::symlink(&target, &link).unwrap();
        assert_eq!(JdkDetector::installation_path(&link).unwrap(), target.canonicalize().unwrap());
    }
}
