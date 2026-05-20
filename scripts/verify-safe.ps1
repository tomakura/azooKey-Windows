param(
    [switch]$SkipFrontendBuild,
    [switch]$SkipX86
)

$ErrorActionPreference = "Stop"

$repo = Resolve-Path (Join-Path $PSScriptRoot "..")
Push-Location $repo
try {
    cargo fmt -- --check
    cargo test -p azookey-converter
    cargo test -p azookey-windows --lib
    cargo check --workspace
    cargo check -p azookey-windows --target x86_64-pc-windows-msvc

    if (-not $SkipX86) {
        cargo check -p azookey-windows --target i686-pc-windows-msvc
    }

    if (-not $SkipFrontendBuild) {
        $npmCli = "C:\Program Files\nodejs\node_modules\npm\bin\npm-cli.js"
        if (Test-Path $npmCli) {
            Push-Location (Join-Path $repo "frontend")
            try {
                node $npmCli run build
            } finally {
                Pop-Location
            }
        } else {
            Push-Location (Join-Path $repo "frontend")
            try {
                npm run build
            } finally {
                Pop-Location
            }
        }
    }

    git diff --check
} finally {
    Pop-Location
}
