<p align="center">
  <img src="assets/hero.svg" alt="jdk-switch：在终端列出、下载和切换 JDK" width="100%">
</p>

<p align="center">
  <strong>发现本机 JDK，选择发行版，在终端切换 Java。</strong>
</p>

<p align="center">
  <a href="https://github.com/nanocm/jdk-switch/releases/latest">下载</a> ·
  <a href="README.md">English</a> ·
  <a href="RELEASE_NOTES.md">更新说明</a>
</p>

## 安装

安装脚本会下载适合当前平台的发布包，校验 SHA-256；如果安装目录不在 `PATH` 中，会先询问是否写入用户级 `PATH`。

**Windows x64 · PowerShell**

```powershell
Invoke-WebRequest https://github.com/nanocm/jdk-switch/releases/latest/download/install.ps1 -OutFile install.ps1 -UseBasicParsing
powershell -NoProfile -ExecutionPolicy Bypass -File install.ps1
```

**Linux x64 · macOS x64 / ARM64**

```bash
curl -fsSL https://github.com/nanocm/jdk-switch/releases/latest/download/install.sh -o install.sh
bash install.sh
```

Windows 默认安装到 `%LOCALAPPDATA%\Programs\jsh`，Unix 默认安装到 `~/.local/bin`。可用 `-InstallDir 'D:\Tools\jsh'` 或 `--dir "$HOME/tools"` 指定用户可写目录。无人值守安装可加 `-PathAction Add` / `--add-to-path`；自行配置 `PATH` 时可加 `-PathAction Skip` / `--skip-path`。安装器修改 `PATH` 后请新开终端。也可[手动下载发布包](https://github.com/nanocm/jdk-switch/releases/latest)。

## 快速开始

```console
jsh list          # 查找已安装的 JDK
jsh download 21   # 安装 Eclipse Temurin 21（默认）
jsh use 21        # 切换版本
jsh current       # 检查当前 Java
```

**第一次**运行 `jsh use` 后，让终端重新加载一次 `jsh-current/bin`：

| Windows | zsh | Linux Bash | macOS Bash |
| :--- | :--- | :--- | :--- |
| 重新打开终端。 | `source ~/.zshrc` | `source ~/.bashrc` | `source ~/.bash_profile` |

此后在同一个终端运行 `jsh use` 即可切换。如果 `PATH` 中其他 Java 排在前面，`jsh current` 会提示路径不一致。

## 选择 JDK 发行版

```console
jsh search 21 --vendor zulu
jsh download 21 --vendor corretto
jsh download 21 --vendor zulu
jsh download 21 --vendor openjdk
```

| `--vendor` | 发行版 | 安装包来源 |
| :--- | :--- | :--- |
| `temurin`（默认） | Eclipse Temurin | Eclipse Adoptium |
| `corretto` | Amazon Corretto | Amazon Corretto |
| `zulu` | Azul Zulu | Azul 元数据 API 与 CDN |
| `openjdk` | 官方 OpenJDK 构建 | `jdk.java.net` |

官方 `openjdk` 安装包来自 [jdk.java.net](https://jdk.java.net/)（JDK 12 及更新版本）。旧版本构建已不再接收安全修复，`jsh` 会将其标记为 **archived**。

主下载地址失败时，Temurin 会尝试[清华 Adoptium 镜像](https://mirrors.tuna.tsinghua.edu.cn/help/Adoptium/)，OpenJDK 会尝试[华为云 OpenJDK 镜像](https://mirrors.huaweicloud.com/openjdk/)。镜像包是否可用取决于版本和平台；每个下载文件都必须通过原始大小和 SHA-256 校验。

## 自定义 JDK 目录

在 `jsh` 可执行文件旁创建或编辑 `jsh_config.json`：

```json
{
  "install_dir": "managed-jdks",
  "download_dir": "downloads",
  "scan_dirs": []
}
```

| 配置项 | 用途 |
| :--- | :--- |
| `install_dir` | `jsh download` 解压安装 JDK 的目录；默认是 `jdks`。 |
| `download_dir` | 已校验压缩包的缓存目录；默认是 `downloads`。 |
| `scan_dirs` | `jsh list` 额外检查的已有目录；默认不添加。 |

相对路径以可执行文件所在目录为基准；也支持绝对路径，例如 Windows 的 `D:\Java\JDKs` 或 Unix 的 `/opt/jdks`。修改 `install_dir` 不会移动已有 JDK；`jsh list` 仍会检查原来的 `jdks` 目录。

首次运行时会自动导入旧版 jsh 的 `config.json`，旧文件保持原样；此后优先读取 `jsh_config.json`。

## 命令

| 命令 | 用途 |
| :--- | :--- |
| `jsh list` | 列出已登记的 JDK，并检查托管目录、`scan_dirs` 和 `JAVA_HOME`。 |
| `jsh list --scan` | 额外搜索常见系统位置。 |
| `jsh list --prune` | 清理路径已失效的登记项，不删除 JDK 文件。 |
| `jsh search [关键词] [--vendor 名称]` | 搜索指定发行版的可下载版本。 |
| `jsh download <主版本> [--vendor 名称]` | 下载、安装并登记 JDK。 |
| `jsh use <版本或ID>` | 切换到已安装的 JDK。 |
| `jsh current` | 显示当前 Java，并检查 `PATH` 是否一致。 |
| `jsh help <命令>` | 查看命令参数和用法示例。 |

相同版本的多个安装会保留不同 ID。版本有歧义时，请使用 `jsh list` 显示的 ID。

<details>
<summary>从源码构建</summary>

需要 Rust 1.88 或更新版本。

```console
cargo test --locked
cargo build --release --locked
```

</details>

<p align="center">
  基于 <a href="https://github.com/YiTouch/j-switch">YiTouch/j-switch</a> fork · <a href="LICENSE">MIT 许可证</a>
</p>
