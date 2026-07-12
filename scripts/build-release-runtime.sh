#!/usr/bin/env sh
set -eu

if [ "$#" -ne 1 ]; then
  echo "usage: $0 <rust-target>" >&2
  exit 2
fi

target="$1"
target_dir="${CARGO_TARGET_DIR:-target}"
manifest="runtime-bundles/$target/yar-runtime.toml"
if [ ! -f "$manifest" ]; then
  echo "unsupported release runtime target: $target" >&2
  exit 2
fi
archive="$(sed -n 's/^archive = "\([^"]*\)"$/\1/p' "$manifest")"
if [ -z "$archive" ]; then
  echo "runtime bundle manifest does not declare an archive: $manifest" >&2
  exit 1
fi

# cargo-zigbuild selects Zig for Rust's linker, while cc-based build scripts
# require their own explicit target compiler and archiver selection.
export CRATE_CC_NO_DEFAULTS=1
case "$target" in
  x86_64-apple-darwin)
    export CC_x86_64_apple_darwin="$PWD/scripts/zig-cc.sh"
    export CFLAGS_x86_64_apple_darwin="-target x86_64-macos"
    export AR_x86_64_apple_darwin="zig ar"
    ;;
  aarch64-apple-darwin)
    export CC_aarch64_apple_darwin="$PWD/scripts/zig-cc.sh"
    export CFLAGS_aarch64_apple_darwin="-target aarch64-macos"
    export AR_aarch64_apple_darwin="zig ar"
    ;;
  x86_64-unknown-linux-gnu)
    export CC_x86_64_unknown_linux_gnu="$PWD/scripts/zig-cc.sh"
    export CFLAGS_x86_64_unknown_linux_gnu="-target x86_64-linux-gnu"
    export AR_x86_64_unknown_linux_gnu="zig ar"
    ;;
  aarch64-unknown-linux-gnu)
    export CC_aarch64_unknown_linux_gnu="$PWD/scripts/zig-cc.sh"
    export CFLAGS_aarch64_unknown_linux_gnu="-target aarch64-linux-gnu"
    export AR_aarch64_unknown_linux_gnu="zig ar"
    ;;
  x86_64-pc-windows-gnu)
    export CC_x86_64_pc_windows_gnu="$PWD/scripts/zig-cc.sh"
    export CFLAGS_x86_64_pc_windows_gnu="-target x86_64-windows-gnu"
    export AR_x86_64_pc_windows_gnu="zig ar"
    ;;
esac

cargo zigbuild -p yar-runtime --release --target "$target"

out_dir="dist/runtime/$target"
native_output="$(cargo rustc -p yar-runtime --release --target "$target" -- --print native-static-libs 2>&1)" || {
  printf '%s\n' "$native_output" >&2
  exit 1
}
native_libraries="$(printf '%s\n' "$native_output" | sed -n 's/^note: native-static-libs: //p')"
declared_libraries="$(
  sed -n 's/^system_libraries = \[\(.*\)\]$/\1/p' "$manifest" |
    tr -d '" ' |
    tr ',' '\n' |
    sed 's/^/-l/' |
    paste -sd ' ' -
)"
if [ -z "$native_libraries" ] || [ "$native_libraries" != "$declared_libraries" ]; then
  echo "runtime bundle libraries for $target do not match rustc" >&2
  echo "declared: $declared_libraries" >&2
  echo "rustc:    $native_libraries" >&2
  exit 1
fi

mkdir -p "$out_dir"
cp "$target_dir/$target/release/$archive" "$out_dir/$archive"
cp "$manifest" "$out_dir/yar-runtime.toml"
