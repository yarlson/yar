# Standard Library

## Design

- The standard library is written in yar, not C or Go.
- Stdlib packages are embedded into the Go compiler binary using `go:embed` via the `internal/stdlib` package.
- Stdlib packages are imported with bare paths like any user package: `import "strings"`.
- Resolution order: local packages first, embedded stdlib second. A local directory with the same name shadows the stdlib package.
- Stdlib packages are parsed, type-checked, and compiled through the same pipeline as user code — no special handling in the checker or codegen.

## Infrastructure

- `internal/stdlib/stdlib.go` provides `Has`, `ReadDir`, and `ReadFile` for the package loader.
- `internal/stdlib/packages/<pkg>/<file>.yar` is the canonical location for stdlib source files.
- `internal/compiler/packages.go` calls `loadStdlibPackage` as a fallback when `os.ReadDir` fails with `os.ErrNotExist` for an import path.
- Both local and stdlib packages share the same `resolveImports` method for import resolution.
- Stdlib packages can import other stdlib packages.

## Packages

### `strings`

Practical string operations built on proposal 0006 primitives (`len(str)`, `s[i]`, `s[i:j]`, `==`, `+`).

Functions:

- `contains(s str, substr str) bool` — linear scan with slice compare
- `has_prefix(s str, prefix str) bool` — compare prefix slice
- `has_suffix(s str, suffix str) bool` — compare suffix slice
- `index(s str, substr str) i32` — byte offset or -1
- `count(s str, substr str) i32` — non-overlapping occurrences
- `repeat(s str, n i32) str` — concatenation loop
- `replace(s str, old str, new str, n i32) str` — find-and-replace, n < 0 means all
- `trim_left(s str, cutset str) str` — strip leading bytes in cutset
- `trim_right(s str, cutset str) str` — strip trailing bytes in cutset
- `join(parts []str, sep str) str` — join slice of strings

Internal helpers `contains_byte` and `parse_positive` are not exported.

Additional functions (proposal 0008):

- `from_byte(i32) str` — construct a single-byte string (wraps `chr` builtin)
- `parse_i64(str) !i64` — parse a base-10 signed integer; returns `error.InvalidInteger` or `error.IntegerOverflow`

### `utf8`

UTF-8 decoding and rune classification for lexers and diagnostic code.

Functions:

- `decode(s str, off i32) !i32` — decode the rune at byte offset `off`
- `width(s str, off i32) !i32` — byte width of the rune at byte offset `off`
- `is_letter(r i32) bool` — letter or underscore classification (ASCII plus common Unicode letter ranges)
- `is_digit(r i32) bool` — ASCII digit 0–9
- `is_space(r i32) bool` — Unicode whitespace codepoints

Errors: `error.InvalidUTF8` for malformed sequences, `error.OutOfRange` for invalid offsets.

### `conv`

Integer-to-string conversion.

Functions:

- `itoa(n i32) str` — base-10 decimal string from i32
- `itoa64(n i64) str` — base-10 decimal string from i64

Depends on `strings.from_byte` for digit character construction.

## Constraints

- All stdlib functions use only the public language surface plus compiler builtins (`chr`, `i32_to_i64`, `i64_to_i32`). No compiler-internal backdoors.
- Performance is naive and correct. Concatenation-heavy functions like `repeat`, `replace`, `itoa`, and `itoa64` are O(n^2) for large inputs — acceptable for the current stage.
- Stdlib packages are not versioned separately from the compiler.

## Adding a New Package

1. Create `internal/stdlib/packages/<name>/<name>.yar` declaring `package <name>`.
2. Mark exported functions with `pub`.
3. No Go code changes needed — the embed and loader handle discovery automatically.
4. Add integration tests in `internal/compiler/compiler_test.go` and a fixture in `testdata/`.
5. Document in `docs/YAR.md` under the Standard Library section.

## Testing

- `internal/stdlib/stdlib_test.go` covers embedding: `Has`, `ReadDir`, `ReadFile`.
- `internal/compiler/compiler_test.go` covers end-to-end: `TestStdlibStringsFixtureProgram`, `TestStdlibStringsExtFixtureProgram`, `TestStdlibUTF8FixtureProgram`, and `TestStdlibConvFixtureProgram` compile and run programs using stdlib functions.
- `TestLocalPackageShadowsStdlib` verifies the shadowing behavior.
- `testdata/stdlib_strings/main.yar`, `testdata/stdlib_strings_ext/main.yar`, `testdata/stdlib_utf8/main.yar`, and `testdata/stdlib_conv/main.yar` are the representative fixtures.
