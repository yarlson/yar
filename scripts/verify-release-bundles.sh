#!/usr/bin/env sh
set -eu

dist_dir="${1:-dist}"

verify_tar() {
  os="$1"
  arch="$2"
  target="$3"
  archive="$(find "$dist_dir" -maxdepth 1 -type f -name "yar_*_${os}_${arch}.tar.gz" -print)"
  if [ -z "$archive" ] || [ "$(printf '%s\n' "$archive" | wc -l | tr -d ' ')" -ne 1 ]; then
    echo "expected one ${os}/${arch} release archive" >&2
    exit 1
  fi
  verify_entries "$(tar -tzf "$archive")" "$target" yar
  tar -xOf "$archive" "runtimes/$target/yar-runtime.toml" |
    cmp - "runtime-bundles/$target/yar-runtime.toml"
}

verify_zip() {
  os="$1"
  arch="$2"
  target="$3"
  archive="$(find "$dist_dir" -maxdepth 1 -type f -name "yar_*_${os}_${arch}.zip" -print)"
  if [ -z "$archive" ] || [ "$(printf '%s\n' "$archive" | wc -l | tr -d ' ')" -ne 1 ]; then
    echo "expected one ${os}/${arch} release archive" >&2
    exit 1
  fi
  verify_entries "$(unzip -Z1 "$archive")" "$target" yar.exe
  unzip -p "$archive" "runtimes/$target/yar-runtime.toml" |
    cmp - "runtime-bundles/$target/yar-runtime.toml"
}

verify_entries() {
  entries="$1"
  target="$2"
  binary="$3"
  expected="runtimes/$target/libyar_runtime.a
runtimes/$target/yar-runtime.toml
$binary"
  if [ "$(printf '%s\n' "$entries" | sort)" != "$(printf '%s\n' "$expected" | sort)" ]; then
    echo "unexpected release contents for $target:" >&2
    printf '%s\n' "$entries" >&2
    exit 1
  fi
}

verify_tar darwin amd64 x86_64-apple-darwin
verify_tar darwin arm64 aarch64-apple-darwin
verify_tar linux amd64 x86_64-unknown-linux-gnu
verify_tar linux arm64 aarch64-unknown-linux-gnu
verify_zip windows amd64 x86_64-pc-windows-gnu

host_archive_pattern=""
case "$(uname -s)/$(uname -m)" in
  Darwin/arm64) host_archive_pattern='yar_*_darwin_arm64.tar.gz' ;;
  Darwin/x86_64) host_archive_pattern='yar_*_darwin_amd64.tar.gz' ;;
  Linux/x86_64) host_archive_pattern='yar_*_linux_amd64.tar.gz' ;;
  Linux/aarch64) host_archive_pattern='yar_*_linux_arm64.tar.gz' ;;
esac
if [ -n "$host_archive_pattern" ]; then
  archive="$(find "$dist_dir" -maxdepth 1 -type f -name "$host_archive_pattern" -print)"
  smoke_dir="$(mktemp -d)"
  trap 'rm -rf "$smoke_dir"' EXIT
  tar -xzf "$archive" -C "$smoke_dir"
  "$smoke_dir/yar" build testdata/hello/main.yar -o "$smoke_dir/hello"
  if [ "$("$smoke_dir/hello")" != "hello, world" ]; then
    echo "packaged compiler failed runtime-bundle smoke build" >&2
    exit 1
  fi
fi

echo "verified target runtime bundles in release archives"
