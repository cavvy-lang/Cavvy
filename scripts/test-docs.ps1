param(
    [switch]$NoBuild,
    [switch]$KeepTemp
)

$ErrorActionPreference = "Stop"
$repo = Resolve-Path (Join-Path $PSScriptRoot "..")

if (-not $NoBuild) {
    cargo build --release
}

$argsList = @((Join-Path $repo "scripts/doc-test.py"))
if ($KeepTemp) {
    $argsList += "--keep-temp"
}

python @argsList
