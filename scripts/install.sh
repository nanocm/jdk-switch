#!/usr/bin/env bash
set -euo pipefail

repo='nanocm/jdk-switch'
install_dir="${HOME}/.local/bin"
version=''
path_action='prompt'
path_option_count=0

usage() {
  cat <<'EOF'
Install a jdk-switch GitHub release for Linux x64 or macOS x64/ARM64.

Usage: bash install.sh [--dir DIRECTORY] [--version TAG]
                       [--add-to-path | --skip-path]

The default directory is ~/.local/bin. If it is missing from PATH, the
installer asks before adding it to your bash or zsh startup file.
EOF
}

fail() {
  printf 'Error: %s\n' "$*" >&2
  exit 1
}

while (($#)); do
  case "$1" in
    --dir|--version)
      (($# >= 2)) || fail "$1 requires a value"
      if [[ $1 == --dir ]]; then install_dir=$2; else version=$2; fi
      shift 2
      ;;
    --add-to-path) path_action='add'; ((path_option_count+=1)); shift ;;
    --skip-path) path_action='skip'; ((path_option_count+=1)); shift ;;
    -h|--help) usage; exit 0 ;;
    *) fail "Unknown option: $1" ;;
  esac
done

[[ -n $install_dir ]] || fail 'Installation directory cannot be empty'
[[ $install_dir != *:* && $install_dir != *$'\n'* ]] || fail 'Installation directory cannot contain a colon or newline'
((path_option_count <= 1)) || fail 'Choose only one PATH option'
command -v curl >/dev/null 2>&1 || fail 'curl is required'
command -v tar >/dev/null 2>&1 || fail 'tar is required'
command -v install >/dev/null 2>&1 || fail 'install is required'

case "$(uname -s)-$(uname -m)" in
  Linux-x86_64) platform='linux-x64-x86_64' ;;
  Darwin-x86_64) platform='macos-x64' ;;
  Darwin-arm64) platform='macos-arm64' ;;
  *) fail 'No published binary is available for this operating system and CPU' ;;
esac

case $install_dir in
  '~') install_dir=$HOME ;;
  '~/'*) install_dir=$HOME/${install_dir#\~/} ;;
esac
mkdir -p -- "$install_dir"
install_dir=$(cd -P -- "$install_dir" && pwd -P)

path_contains_dir() {
  local remaining=${PATH:-} entry resolved
  while :; do
    entry=${remaining%%:*}
    if [[ -n $entry && -d $entry ]]; then
      resolved=$(cd -P -- "$entry" 2>/dev/null && pwd -P) || resolved=''
      [[ $resolved == "$install_dir" ]] && return 0
    fi
    [[ $remaining == *:* ]] || break
    remaining=${remaining#*:}
  done
  return 1
}

profile=''
if [[ $path_action == add ]] || ! path_contains_dir; then
  if [[ $path_action == prompt ]]; then
    [[ -r /dev/tty ]] || fail "Directory is not on PATH. Re-run with --add-to-path or --skip-path: $install_dir"
    printf 'Add %s to your user PATH? [Y/n] ' "$install_dir" >/dev/tty
    IFS= read -r answer </dev/tty || fail 'Could not read PATH choice'
    case $answer in
      ''|y|Y|yes|YES) path_action='add' ;;
      n|N|no|NO) path_action='skip' ;;
      *) fail 'Please answer yes or no' ;;
    esac
  fi
  if [[ $path_action == add ]]; then
    case ${SHELL:-} in
      */zsh|zsh) profile="$HOME/.zshrc" ;;
      */bash|bash)
        if [[ $(uname -s) == Darwin ]]; then profile="$HOME/.bash_profile"; else profile="$HOME/.bashrc"; fi
        ;;
      *) fail 'Automatic PATH setup supports bash and zsh. Use --skip-path and configure your shell manually.' ;;
    esac
  fi
fi

if [[ -z $version ]]; then
  latest_url=$(curl -fsSL -o /dev/null -w '%{url_effective}' "https://github.com/$repo/releases/latest")
  [[ $latest_url =~ ^https://github\.com/nanocm/(j-switch|jdk-switch)/releases/tag/(v[^/]+)$ ]] || fail "Could not identify the latest release: $latest_url"
  repo="nanocm/${BASH_REMATCH[1]}"
  version=${BASH_REMATCH[2]}
fi
[[ $version =~ ^v[0-9]+\.[0-9]+\.[0-9]+(-[A-Za-z0-9.-]+)?$ ]] || fail "Invalid release tag: $version"

asset="jsh-$version-$platform.tar.gz"
release_url="https://github.com/$repo/releases/download/$version"
temp_dir=$(mktemp -d "${TMPDIR:-/tmp}/jsh-install.XXXXXXXX")
cleanup() {
  if [[ -n ${temp_dir:-} && -d $temp_dir && $temp_dir == "${TMPDIR:-/tmp}"/jsh-install.* ]]; then
    rm -rf -- "$temp_dir"
  fi
}
trap cleanup EXIT

printf 'Downloading %s...\n' "$asset"
curl -fsSL --retry 2 -o "$temp_dir/$asset" "$release_url/$asset"
curl -fsSL --retry 2 -o "$temp_dir/SHA256SUMS.txt" "$release_url/SHA256SUMS.txt"
expected=$(awk -v name="$asset" '$2 == name || $2 == "*" name { print $1 }' "$temp_dir/SHA256SUMS.txt")
[[ $expected =~ ^[A-Fa-f0-9]{64}$ ]] || fail "Missing or invalid checksum for $asset"
if command -v sha256sum >/dev/null 2>&1; then
  actual=$(sha256sum "$temp_dir/$asset")
else
  command -v shasum >/dev/null 2>&1 || fail 'sha256sum or shasum is required'
  actual=$(shasum -a 256 "$temp_dir/$asset")
fi
actual=${actual%% *}
actual=$(printf '%s' "$actual" | tr '[:upper:]' '[:lower:]')
expected=$(printf '%s' "$expected" | tr '[:upper:]' '[:lower:]')
[[ $actual == "$expected" ]] || fail "SHA-256 mismatch for $asset"

tar -xzf "$temp_dir/$asset" -C "$temp_dir" jsh
[[ -f $temp_dir/jsh ]] || fail 'Release archive does not contain jsh'
staged="$install_dir/.jsh-install-$$"
trap 'rm -f -- "${staged:-}"; cleanup' EXIT
install -m 755 "$temp_dir/jsh" "$staged"
mv -f -- "$staged" "$install_dir/jsh"

if [[ -n $profile ]]; then
  quoted="'$(printf '%s' "$install_dir" | sed "s/'/'\\\\''/g")'"
  path_line="export PATH=$quoted:\"\$PATH\""
  touch -- "$profile"
  if ! grep -Fqx -- "$path_line" "$profile"; then
    printf '\n# Added by the jdk-switch installer\n%s\n' "$path_line" >> "$profile"
  fi
  printf 'Added %s to %s. Reload it or open a new terminal.\n' "$install_dir" "$profile"
elif ! path_contains_dir; then
  printf 'PATH was not changed. Add %s to PATH to run jsh by name.\n' "$install_dir"
fi

printf 'Installed %s to %s\n' "$version" "$install_dir/jsh"
"$install_dir/jsh" --version
