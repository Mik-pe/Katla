#!/bin/sh
# Bootstrap the verified compiler before an Odin build tool can run.
set -eu
katla_script_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
katla_destination=${1:-target/odin-toolchain}
case $(uname -s) in Darwin) katla_platform=macos ;; Linux) katla_platform=linux ;; *) echo 'Use install_odin.ps1 on Windows.' >&2; exit 1 ;; esac
case $(uname -m) in arm64|aarch64) katla_arch=arm64 ;; x86_64|amd64) katla_arch=amd64 ;; *) echo 'Unsupported Odin architecture.' >&2; exit 1 ;; esac
katla_archive=odin-$katla_platform-$katla_arch-dev-2026-09.tar.gz
katla_expected=$(awk -v file="$katla_archive" '$2 == file {print $1}' "$katla_script_dir/odin-releases.sha256")
test -n "$katla_expected"
mkdir -p "$katla_destination"
katla_destination=$(CDPATH= cd -- "$katla_destination" && pwd)
if [ ! -f "$katla_destination/$katla_archive" ]; then
    curl --fail --location --retry 3 --connect-timeout 30 --output "$katla_destination/$katla_archive.download" "https://github.com/odin-lang/Odin/releases/download/dev-2026-09/$katla_archive"
    mv "$katla_destination/$katla_archive.download" "$katla_destination/$katla_archive"
fi
if command -v sha256sum >/dev/null 2>&1; then
    katla_actual=$(sha256sum "$katla_destination/$katla_archive" | awk '{print $1}')
else
    katla_actual=$(shasum -a 256 "$katla_destination/$katla_archive" | awk '{print $1}')
fi
if [ "$katla_actual" != "$katla_expected" ]; then echo 'Odin release checksum mismatch.' >&2; exit 1; fi
mkdir -p "$katla_destination/dev-2026-09"
tar -xzf "$katla_destination/$katla_archive" -C "$katla_destination/dev-2026-09"
katla_compiler=$(find "$katla_destination/dev-2026-09" -type f -name odin)
if [ "$(printf '%s\n' "$katla_compiler" | wc -l | tr -d ' ')" != 1 ] || [ ! -f "$katla_compiler" ]; then echo 'Expected exactly one Odin executable.' >&2; exit 1; fi
chmod +x "$katla_compiler"
printf '%s\n' "$katla_compiler"
