$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

$target = "x86_64-pc-windows-gnu"
$targetDir = Join-Path $PWD "target/windows-concurrency"
$bundle = Join-Path $targetDir "release/runtime-bundle"
$yar = Join-Path $targetDir "debug/yar.exe"
$runtime = Join-Path $targetDir "release/libyar_runtime.a"

$env:RUSTUP_TOOLCHAIN = "stable-$target"
$env:CARGO_TARGET_DIR = $targetDir

cargo build -p yar-cli
if ($LASTEXITCODE -ne 0) {
    throw "failed to build the Yar CLI"
}
cargo build -p yar-runtime --release
if ($LASTEXITCODE -ne 0) {
    throw "failed to build the Yar runtime"
}
foreach ($filter in @("concurrency::", "memory::", "taskgroup_helpers", "fatal_worker", "output_lock")) {
    cargo test -p yar-runtime --release $filter
    if ($LASTEXITCODE -ne 0) {
        throw "failed to run Yar runtime tests matching $filter"
    }
}

New-Item -ItemType Directory -Force -Path $bundle | Out-Null
Copy-Item $runtime (Join-Path $bundle "libyar_runtime.a")
Copy-Item "runtime-bundles/$target/yar-runtime.toml" (Join-Path $bundle "yar-runtime.toml")

$env:YAR_RUNTIME_BUNDLE = $bundle
$env:YAR_GC_HEAP_TARGET_BYTES = "1024"

$fixtures = @(
    @{ Name = "concurrency_basic"; Expected = "4`n9" },
    @{ Name = "concurrency_channels"; Expected = "13" },
    @{ Name = "concurrency_errors"; Expected = "1`nerror.Zero" },
    @{ Name = "concurrency_fs"; Expected = "hello`nhello" },
    @{ Name = "concurrency_lifecycle"; Expected = "251000" },
    @{ Name = "concurrency_share_safe"; Expected = "" },
    @{ Name = "garbage_collection"; Expected = "142000" }
)

foreach ($fixture in $fixtures) {
    $source = "testdata/$($fixture.Name)/main.yar"
    $executable = Join-Path $targetDir "$($fixture.Name).exe"
    & $yar build $source -o $executable
    if ($LASTEXITCODE -ne 0) {
        throw "failed to build $source"
    }

    $actual = @(& $executable) -join "`n"
    if ($LASTEXITCODE -ne 0) {
        throw "$source exited with status $LASTEXITCODE"
    }
    if ($actual -ne $fixture.Expected) {
        throw "$source produced unexpected stdout: $actual"
    }
}

Write-Output "ran $($fixtures.Count) concurrency and collection fixtures on Windows"
