#!/usr/bin/env sh
set -eu

target_dir="${CARGO_TARGET_DIR:-target}"
yar_bin="$target_dir/debug/yar"
runtime_archive="$target_dir/release/libyar_runtime.a"

cargo build -p yar-cli
cargo build -p yar-runtime --release

if [ ! -x "$yar_bin" ]; then
  echo "yar binary not found at $yar_bin" >&2
  exit 1
fi
if [ ! -f "$runtime_archive" ]; then
  echo "runtime archive not found at $runtime_archive" >&2
  exit 1
fi

fixtures="$(mktemp)"
trap 'rm -f "$fixtures"' EXIT
find testdata -name main.yar | sort > "$fixtures"

count=0
while IFS= read -r fixture; do
  output="$(mktemp -t yar-rust-fixture.XXXXXX)"
  YAR_RUNTIME_ARCHIVE="$runtime_archive" "$yar_bin" build "$fixture" -o "$output"
  rm -f "$output"
  count=$((count + 1))
done < "$fixtures"

printf 'built %d testdata fixtures with Rust CLI\n' "$count"
