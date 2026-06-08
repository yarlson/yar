#!/usr/bin/env sh
set -eu

if [ "$#" -ne 1 ]; then
  echo "usage: $0 <rust-target>" >&2
  exit 2
fi

target="$1"
archive="libyar_runtime.a"
if [ "$target" = "x86_64-pc-windows-msvc" ]; then
  archive="yar_runtime.lib"
fi

cargo zigbuild -p yar-runtime --release --target "$target"

out_dir="dist/runtime/$target"
mkdir -p "$out_dir"
cp "target/$target/release/$archive" "$out_dir/$archive"
