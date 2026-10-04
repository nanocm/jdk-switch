use crate::config::Config;
use crate::env::EnvUpdater;
use crate::error::{JdkError, Result};
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

pub struct UnixEnvUpdater;

impl UnixEnvUpdater {
    pub fn new() -> Self { Self }

    fn link_path() -> Result<PathBuf> {
        Ok(Config::config_dir()?.join("jsh-current"))
    }

    pub(crate) fn shell_rc_path() -> Result<PathBuf> {
        let home = dirs::home_dir()
            .ok_or_else(|| JdkError::EnvError("Cannot find home directory".to_string()))?;
        let shell = std::env::var_os("SHELL")
            .and_then(|shell| PathBuf::from(shell).file_name().map(|name| name.to_owned()))
            .as_deref()
            .and_then(|name| name.to_str())
            .map(str::to_owned);
        Ok(home.join(Self::shell_profile_name(shell.as_deref())?))
    }

    fn shell_profile_name(shell: Option<&str>) -> Result<&'static str> {
        match shell {
            Some("zsh") => Ok(".zshrc"),
            Some("bash") | None => {
                #[cfg(target_os = "macos")]
                { Ok(".bash_profile") }
                #[cfg(not(target_os = "macos"))]
                { Ok(".bashrc") }
            }
            Some(shell) => Err(JdkError::EnvError(format!(
                "Shell {shell} is not supported; use bash or zsh to configure automatic Java switching"
            ))),
        }
    }

    /// Replace only a symlink that belongs at the managed location. The rename
    /// is atomic, so other shells never observe a half-written target.
    fn switch_symlink_at(link: &Path, target: &Path) -> Result<Option<PathBuf>> {
        let old_target = match fs::symlink_metadata(link) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(error) => return Err(JdkError::EnvError(format!(
                "Cannot inspect {}: {error}", link.display()
            ))),
            Ok(metadata) if metadata.file_type().is_symlink() => Some(fs::read_link(link)?),
            Ok(_) => return Err(JdkError::EnvError(format!(
                "{} already exists and is not a symlink", link.display()
            ))),
        };
        let parent = link.parent().ok_or_else(|| JdkError::EnvError(
            "JDK symlink has no parent directory".to_string()
        ))?;
        let staging = tempfile::Builder::new().prefix(".jsh-link-").tempdir_in(parent)?;
        let staged_link = staging.path().join("current");
        std::os::unix::fs::symlink(target, &staged_link)?;
        fs::rename(&staged_link, link).map_err(|error| JdkError::EnvError(format!(
            "Cannot activate JDK symlink {}: {error}", link.display()
        )))?;
        Ok(old_target)
    }

    fn restore_symlink_at(link: &Path, old_target: Option<&Path>) -> Result<()> {
        if let Some(target) = old_target {
            Self::switch_symlink_at(link, target)?;
        } else if fs::symlink_metadata(link).is_ok() {
            fs::remove_file(link)?;
        }
        Ok(())
    }

    fn update_shell_rc_at(&self, rc_path: &Path, java_home: &Path) -> Result<()> {
        let java_home_str = java_home.to_str()
            .ok_or_else(|| JdkError::EnvError("JDK path is not valid UTF-8".to_string()))?;
        let quoted_home = Self::shell_quote(java_home_str);
        let write_path = if rc_path.exists() { rc_path.canonicalize()? } else { rc_path.to_path_buf() };
        let mut lines = Vec::new();
        let mut found_java_home = false;
        let mut found_path = false;

        if write_path.exists() {
            let reader = BufReader::new(fs::File::open(&write_path)?);
            for line in reader.lines() {
                let line = line?;
                if line.trim_start().starts_with("export JAVA_HOME=") && line.contains("# jsh managed") {
                    if !found_java_home {
                        lines.push(format!("export JAVA_HOME={quoted_home}  # jsh managed"));
                        found_java_home = true;
                    }
                } else if line.trim_start().starts_with("export PATH=") && line.contains("# jsh managed") {
                    if !found_path {
                        lines.push("export PATH=\"$JAVA_HOME/bin:$PATH\"  # jsh managed".to_string());
                        found_path = true;
                    }
                } else {
                    lines.push(line);
                }
            }
        }
        if !found_java_home {
            lines.push("".to_string());
            lines.push("# jsh managed - do not edit manually".to_string());
            lines.push(format!("export JAVA_HOME={quoted_home}  # jsh managed"));
        }
        if !found_path {
            lines.push("export PATH=\"$JAVA_HOME/bin:$PATH\"  # jsh managed".to_string());
        }

        let parent = write_path.parent().ok_or_else(|| JdkError::EnvError(
            "Shell profile has no parent directory".to_string()
        ))?;
        let mut file = tempfile::NamedTempFile::new_in(parent)?;
        if let Ok(metadata) = fs::metadata(&write_path) {
            file.as_file().set_permissions(metadata.permissions())?;
        }
        for line in lines { writeln!(file, "{line}")?; }
        file.flush()?;
        file.as_file().sync_all()?;
        file.persist(&write_path).map_err(|error| JdkError::EnvError(format!(
            "Cannot replace shell profile: {}", error.error
        )))?;

        println!("[OK] Updated {}", rc_path.display());
        Ok(())
    }

    pub(crate) fn shell_quote(value: &str) -> String {
        format!("'{}'", value.replace('\'', "'\\''"))
    }

    /// A shell initialized with the stable path sees the new JDK immediately.
    pub fn shell_uses_link(&self) -> Result<bool> {
        let link = Self::link_path()?;
        if std::env::var_os("JAVA_HOME").as_deref() != Some(link.as_os_str()) {
            return Ok(false);
        }
        let expected_bin = link.join("bin");
        let first_java_dir = std::env::var_os("PATH")
            .map(|path| std::env::split_paths(&path)
                .find(|entry| entry.join("java").is_file()))
            .flatten();
        Ok(first_java_dir.as_deref() == Some(expected_bin.as_path()))
    }
}

impl EnvUpdater for UnixEnvUpdater {
    fn update_java_home(&self, path: &Path) -> Result<()> {
        let target = path.canonicalize().map_err(|error| JdkError::InvalidPath(format!(
            "{}: {error}", path.display()
        )))?;
        let link = Self::link_path()?;
        let rc_path = Self::shell_rc_path()?;
        let old_target = Self::switch_symlink_at(&link, &target)?;
        if let Err(error) = self.update_shell_rc_at(&rc_path, &link) {
            if let Err(restore) = Self::restore_symlink_at(&link, old_target.as_deref()) {
                return Err(JdkError::EnvError(format!(
                    "{error}; symlink restore also failed: {restore}"
                )));
            }
            return Err(error);
        }
        println!("[OK] Active JDK symlink: {} -> {}", link.display(), target.display());
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::UnixEnvUpdater;
    use std::fs;
    use std::io::{BufRead, BufReader, Write};
    use std::os::unix::fs::PermissionsExt;
    use std::process::{Command, Stdio};

    #[test]
    fn shell_path_does_not_expand_special_characters() {
        assert_eq!(UnixEnvUpdater::shell_quote("/tmp/$HOME/`test`/a'b"),
            "'/tmp/$HOME/`test`/a'\\''b'");
    }

    #[test]
    fn chooses_the_shell_startup_file() {
        assert_eq!(UnixEnvUpdater::shell_profile_name(Some("zsh")).unwrap(), ".zshrc");
        #[cfg(target_os = "macos")]
        assert_eq!(UnixEnvUpdater::shell_profile_name(Some("bash")).unwrap(), ".bash_profile");
        #[cfg(not(target_os = "macos"))]
        assert_eq!(UnixEnvUpdater::shell_profile_name(Some("bash")).unwrap(), ".bashrc");
    }

    #[test]
    fn an_open_shell_sees_a_new_jdk_without_resourcing() {
        let temp = tempfile::tempdir().unwrap();
        let profile = temp.path().join(".bashrc");
        let link = temp.path().join("jsh $HOME `literal` a'b");
        let first = temp.path().join("jdk-17");
        let second = temp.path().join("jdk-21");
        for (jdk, label) in [(&first, "17"), (&second, "21")] {
            fs::create_dir_all(jdk.join("bin")).unwrap();
            fs::write(jdk.join("bin/java"), format!("#!/bin/sh\nprintf '{label}\\n'\n")).unwrap();
            fs::set_permissions(jdk.join("bin/java"), fs::Permissions::from_mode(0o755)).unwrap();
        }
        fs::write(&profile, "export OTHER=value\n").unwrap();
        let updater = UnixEnvUpdater::new();
        UnixEnvUpdater::switch_symlink_at(&link, &first).unwrap();
        updater.update_shell_rc_at(&profile, &link).unwrap();
        updater.update_shell_rc_at(&profile, &link).unwrap();
        let content = fs::read_to_string(&profile).unwrap();
        assert_eq!(content.matches("export JAVA_HOME=").count(), 1);
        assert_eq!(content.matches("export PATH=").count(), 1);
        assert!(content.contains("export OTHER=value"));

        let mut child = Command::new("sh").arg("-s").arg(&profile)
            .stdin(Stdio::piped()).stdout(Stdio::piped()).spawn().unwrap();
        let mut input = child.stdin.take().unwrap();
        let mut output = BufReader::new(child.stdout.take().unwrap());
        writeln!(input, ". \"$1\"; java").unwrap();
        input.flush().unwrap();
        let mut line = String::new();
        output.read_line(&mut line).unwrap();
        assert_eq!(line.trim(), "17");

        UnixEnvUpdater::switch_symlink_at(&link, &second).unwrap();
        writeln!(input, "java").unwrap();
        drop(input);
        line.clear();
        output.read_line(&mut line).unwrap();
        assert_eq!(line.trim(), "21");
        assert!(child.wait().unwrap().success());
    }

    #[test]
    fn regular_directory_at_managed_link_is_preserved() {
        let temp = tempfile::tempdir().unwrap();
        let link = temp.path().join("jsh-current");
        fs::create_dir(&link).unwrap();
        fs::write(link.join("keep"), b"user data").unwrap();
        assert!(UnixEnvUpdater::switch_symlink_at(&link, temp.path()).is_err());
        assert_eq!(fs::read(link.join("keep")).unwrap(), b"user data");
    }
}
