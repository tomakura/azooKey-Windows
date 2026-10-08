param(
    [switch]$SkipFrontendBuild,
    [switch]$SkipX86
)

$ErrorActionPreference = "Stop"

function Invoke-Checked {
    param([string]$Command, [string[]]$Arguments)
    & $Command @Arguments
    if ($LASTEXITCODE -ne 0) { throw "$Command failed with exit code $LASTEXITCODE" }
}

$repo = Resolve-Path (Join-Path $PSScriptRoot "..")
Push-Location $repo
try {
    Invoke-Checked cargo @('fmt', '--', '--check')
    Invoke-Checked cargo @('test', '-p', 'shared', '-p', 'azookey-converter', '-p', 'azookey-windows', '--lib')
    Invoke-Checked cargo @('clippy', '--workspace', '--all-targets', '--', '-D', 'warnings')
    Invoke-Checked cargo @('check', '--workspace')
    Invoke-Checked cargo @('check', '-p', 'azookey-windows', '--target', 'x86_64-pc-windows-msvc')

    if (-not $SkipX86) {
        Invoke-Checked cargo @('check', '-p', 'azookey-windows', '--target', 'i686-pc-windows-msvc')
    }

    if (-not $SkipFrontendBuild) {
        $npmCli = "C:\Program Files\nodejs\node_modules\npm\bin\npm-cli.js"
        if (Test-Path $npmCli) {
            Push-Location (Join-Path $repo "frontend")
            try {
                Invoke-Checked node @($npmCli, 'test')
                Invoke-Checked node @($npmCli, 'run', 'build')
            } finally {
                Pop-Location
            }
        } else {
            Push-Location (Join-Path $repo "frontend")
            try {
                Invoke-Checked npm @('test')
                Invoke-Checked npm @('run', 'build')
            } finally {
                Pop-Location
            }
        }
    }

    Invoke-Checked powershell @('-ExecutionPolicy', 'Bypass', '-File', (Join-Path $repo 'scripts/verify-installer-static.ps1'))
    Invoke-Checked git @('diff', '--check')
} finally {
    Pop-Location
}
