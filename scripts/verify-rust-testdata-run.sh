#!/usr/bin/env sh
set -eu

target_dir="${CARGO_TARGET_DIR:-target}"
yar_bin="$target_dir/debug/yar"
runtime_archive="$target_dir/release/libyar_runtime.a"
runtime_bundle="$target_dir/release/runtime-bundle"

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
host_target="$(rustc -vV | sed -n 's/^host: //p')"
mkdir -p "$runtime_bundle"
cp "$runtime_archive" "$runtime_bundle/libyar_runtime.a"
cp "runtime-bundles/$host_target/yar-runtime.toml" "$runtime_bundle/yar-runtime.toml"

tmp_dir="$(mktemp -d)"
fixtures="$tmp_dir/fixtures"
trap 'rm -rf "$tmp_dir"' EXIT

capture_script="$tmp_dir/capture.sh"
inherit_script="$tmp_dir/inherit.sh"
timeout_script="$tmp_dir/timeout.sh"
limit_script="$tmp_dir/limit.sh"
cancel_script="$tmp_dir/cancel.sh"
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
cat >"$timeout_script" <<'EOF'
#!/usr/bin/env sh
sleep 1
EOF
cat >"$limit_script" <<'EOF'
#!/usr/bin/env sh
printf 'four'
EOF
cat >"$cancel_script" <<'EOF'
#!/usr/bin/env sh
sleep 1
EOF
chmod +x "$capture_script" "$inherit_script" "$timeout_script" "$limit_script" "$cancel_script"

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
    testdata/nil_interface/main.yar)
      expected_stderr="nil interface method call"
      ;;
    testdata/nil_pointer/main.yar)
      expected_stderr="runtime failure: nil pointer dereference"
      ;;
  esac

  output="$(mktemp "$tmp_dir/yar-rust-fixture.XXXXXX")"
  if [ "$fixture" = "testdata/garbage_collection/main.yar" ]; then
    ir="$tmp_dir/garbage-collection.ll"
    "$yar_bin" emit-ir "$fixture" >"$ir"
    system_libraries="$(
      sed -n 's/^system_libraries = \[\(.*\)\]$/\1/p' "$runtime_bundle/yar-runtime.toml" |
        tr -d '" ' |
        tr ',' '\n' |
        sed 's/^/-l/' |
        paste -sd ' ' -
    )"
    # shellcheck disable=SC2086 -- manifest library names are validated by the CLI.
    clang -O2 "$ir" "$runtime_archive" $system_libraries -o "$output"
  else
    YAR_RUNTIME_BUNDLE="$runtime_bundle" "$yar_bin" build "$fixture" -o "$output"
  fi

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
    if ! YAR_GC_HEAP_TARGET_BYTES=1024 YAR_PROCESS_ENV_TEST="env ok" \
      "$output" "$capture_script" "$inherit_script" \
      "$timeout_script" "$limit_script" "$cancel_script" >"$stdout" 2>"$stderr"; then
      echo "fixture failed: $fixture" >&2
      echo "stdout:" >&2
      cat "$stdout" >&2
      echo "stderr:" >&2
      cat "$stderr" >&2
      exit 1
    fi
  elif [ "$fixture" = "testdata/garbage_collection/main.yar" ] \
    || [ "$fixture" = "testdata/concurrency_basic/main.yar" ] \
    || [ "$fixture" = "testdata/concurrency_channels/main.yar" ] \
    || [ "$fixture" = "testdata/concurrency_errors/main.yar" ] \
    || [ "$fixture" = "testdata/concurrency_fs/main.yar" ] \
    || [ "$fixture" = "testdata/concurrency_lifecycle/main.yar" ] \
    || [ "$fixture" = "testdata/concurrency_share_safe/main.yar" ]; then
    if ! YAR_GC_HEAP_TARGET_BYTES=1024 "$output" >"$stdout" 2>"$stderr"; then
      echo "fixture failed under forced garbage collection: $fixture" >&2
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
