#!/usr/bin/env bash
set -euo pipefail

original_java_home="${JAVA_HOME:?actions/setup-java must provide JAVA_HOME}"
smoke_root="$(mktemp -d)"
cp target/release/jsh "$smoke_root/jsh"
mkdir -p "$smoke_root/home"
python3 - "$smoke_root/jsh_config.json" "$original_java_home" <<'PY'
import json
import pathlib
import sys

pathlib.Path(sys.argv[1]).write_text(json.dumps({
    "current_jdk": None,
    "jdks": {},
    "download_dir": "downloads",
    "install_dir": "managed-jdks",
    "scan_dirs": [sys.argv[2]],
}), encoding="utf-8")
PY

export HOME="$smoke_root/home"
export SHELL=/bin/bash
export NO_COLOR=1

"$smoke_root/jsh" list > "$smoke_root/list-before.txt"
grep -Fq 'JDK 17' "$smoke_root/list-before.txt"

"$smoke_root/jsh" download 21
python3 - "$smoke_root/jsh_config.json" <<'PY'
import json
import pathlib
import sys

path = pathlib.Path(sys.argv[1])
config = json.loads(path.read_text(encoding="utf-8"))
config["jdks"] = {}
path.write_text(json.dumps(config), encoding="utf-8")
PY
"$smoke_root/jsh" list > "$smoke_root/list-after.txt"
grep -Fq 'JDK 17' "$smoke_root/list-after.txt"
grep -Fq 'JDK 21' "$smoke_root/list-after.txt"
grep -Fq "$smoke_root/managed-jdks" "$smoke_root/list-after.txt"

"$smoke_root/jsh" use 17
source "$HOME/.bashrc"
java -version 2> "$smoke_root/java-17.txt"
grep -Eq 'version "17([.+"]|$)' "$smoke_root/java-17.txt"

"$smoke_root/jsh" use 21
java -version 2> "$smoke_root/java-21.txt"
grep -Eq 'version "21([.+"]|$)' "$smoke_root/java-21.txt"
javac -version > "$smoke_root/javac-21.txt" 2>&1
grep -Eq '^javac 21([.+]|$)' "$smoke_root/javac-21.txt"
"$smoke_root/jsh" current > "$smoke_root/current.txt"
grep -Fq 'JDK 21' "$smoke_root/current.txt"
if grep -Fq 'java on PATH resolves to' "$smoke_root/current.txt"; then
    cat "$smoke_root/current.txt"
    exit 1
fi

if [[ -n "${JSH_EXTRA_VENDOR:-}" ]]; then
    extra_major="${JSH_EXTRA_MAJOR:-21}"
    "$smoke_root/jsh" download "$extra_major" --vendor "$JSH_EXTRA_VENDOR"
    "$smoke_root/jsh" list > "$smoke_root/list-extra.txt"
    grep -Fq "jdk-${JSH_EXTRA_VENDOR}-" "$smoke_root/list-extra.txt"

    extra_id="$(python3 - "$smoke_root/jsh_config.json" "$JSH_EXTRA_VENDOR" <<'PY'
import json
import pathlib
import sys

config = json.loads(pathlib.Path(sys.argv[1]).read_text(encoding="utf-8"))
prefix = f"jdk-{sys.argv[2]}-"
matches = [key for key, info in config["jdks"].items()
           if any(part.startswith(prefix) for part in pathlib.Path(info["path"]).parts)]
if len(matches) != 1:
    raise SystemExit(f"Expected one {sys.argv[2]} registration; found {len(matches)}")
print(matches[0])
PY
)"
    "$smoke_root/jsh" use "$extra_id"
    java -version 2> "$smoke_root/java-extra.txt"
    grep -Eq "version \"${extra_major}([.+\"]|$)" "$smoke_root/java-extra.txt"
    javac -version > "$smoke_root/javac-extra.txt" 2>&1
    grep -Eq "^javac ${extra_major}([.+]|$)" "$smoke_root/javac-extra.txt"
    "$smoke_root/jsh" current > "$smoke_root/current-extra.txt"
    grep -Fq "JDK $extra_major" "$smoke_root/current-extra.txt"
    if grep -Fq 'java on PATH resolves to' "$smoke_root/current-extra.txt"; then
        cat "$smoke_root/current-extra.txt"
        exit 1
    fi
    if [[ "$extra_major" == 21 ]] && "$smoke_root/jsh" use 21 > "$smoke_root/ambiguous.txt" 2>&1; then
        echo 'jsh use 21 accepted multiple installed JDKs without an ID' >&2
        exit 1
    fi
fi

echo "Real JDK download, list, and open-shell switch passed on $(uname -s) $(uname -m)."
