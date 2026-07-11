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

tmp_dir="$(mktemp -d)"
fixtures="$tmp_dir/fixtures"
trap 'rm -rf "$tmp_dir"' EXIT

capture_script="$tmp_dir/capture.sh"
inherit_script="$tmp_dir/inherit.sh"
cat >"$capture_script" <<'EOF'
#!/usr/bin/env sh
printf 'captured stdout\n'
printf 'captured stderr\n' >&2
exit 7
EOF
cat >"$inherit_script" <<'EOF'
#!/usr/bin/env sh
printf 'inherit stdout\n'
printf 'inherit stderr\n' >&2
exit 3
EOF
chmod +x "$capture_script" "$inherit_script"

find testdata -name main.yar | sort >"$fixtures"

count=0
while IFS= read -r fixture; do
  expected_stderr=""
  case "$fixture" in
    testdata/panic/main.yar | testdata/testing_fail/main.yar | testdata/unhandled_error/main.yar)
      continue
      ;;
    testdata/array_bounds/main.yar)
      expected_stderr="runtime failure: array index out of range"
      ;;
    testdata/integer_div_zero/main.yar)
      expected_stderr="runtime failure: integer division or remainder by zero"
      ;;
    testdata/integer_rem_overflow/main.yar)
      expected_stderr="runtime failure: integer division or remainder overflow"
      ;;
    testdata/invalid_string_builder_handle/main.yar)
      expected_stderr="runtime failure: invalid string builder"
      ;;
    testdata/nil_pointer/main.yar)
      expected_stderr="runtime failure: nil pointer dereference"
      ;;
  esac

  output="$(mktemp "$tmp_dir/yar-rust-fixture.XXXXXX")"
  YAR_RUNTIME_ARCHIVE="$runtime_archive" "$yar_bin" build "$fixture" -o "$output"

  stdout="$tmp_dir/stdout"
  stderr="$tmp_dir/stderr"
  if [ -n "$expected_stderr" ]; then
    if "$output" >"$stdout" 2>"$stderr"; then
      echo "fixture unexpectedly succeeded: $fixture" >&2
      exit 1
    fi
    if [ "$(cat "$stderr")" != "$expected_stderr" ]; then
      echo "fixture produced unexpected stderr: $fixture" >&2
      cat "$stderr" >&2
      exit 1
    fi
    continue
  elif [ "$fixture" = "testdata/stdlib_process_env/main.yar" ]; then
    if ! YAR_PROCESS_ENV_TEST="env ok" "$output" "$capture_script" "$inherit_script" >"$stdout" 2>"$stderr"; then
      echo "fixture failed: $fixture" >&2
      echo "stdout:" >&2
      cat "$stdout" >&2
      echo "stderr:" >&2
      cat "$stderr" >&2
      exit 1
    fi
  else
    if ! "$output" >"$stdout" 2>"$stderr"; then
      echo "fixture failed: $fixture" >&2
      echo "stdout:" >&2
      cat "$stdout" >&2
      echo "stderr:" >&2
      cat "$stderr" >&2
      exit 1
    fi
  fi

  count=$((count + 1))
done <"$fixtures"

printf 'ran %d successful testdata fixtures with Rust CLI/runtime\n' "$count"
