$ErrorActionPreference = 'Stop'
$repo = Split-Path $PSScriptRoot -Parent
$checkout = Join-Path $repo 'server-swift/.build/checkouts/AzooKeyKanaKanjiConverter'
$patch = Join-Path $repo 'server-swift/patches/windows-gpu-layers.patch'
$revision = & git -C $checkout rev-parse HEAD
if ($LASTEXITCODE -ne 0 -or $revision -ne 'bbef9d2d99a2e9e69ac3f7e2e07b08474de59a81') {
    throw 'Resolve the pinned Swift dependency before preparing its Windows GPU patch'
}
# An unapplied patch is an expected nonzero result, including under Windows PowerShell 5.
$previousPreference = $ErrorActionPreference
try {
    $ErrorActionPreference = 'Continue'
    & git -C $checkout apply --reverse --check $patch 2>$null
    $alreadyApplied = $LASTEXITCODE -eq 0
} finally {
    $ErrorActionPreference = $previousPreference
}
if ($alreadyApplied) { exit 0 }
& git -C $checkout apply --check $patch
if ($LASTEXITCODE -ne 0) { throw 'Windows GPU patch conflicts with the dependency checkout' }
$source = Join-Path $checkout 'Sources/KanaKanjiConverterModule/ConversionAlgorithms/Zenzai/Zenz/ZenzContext.swift'
(Get-Item -LiteralPath $source).IsReadOnly = $false
& git -C $checkout apply $patch
if ($LASTEXITCODE -ne 0) { throw 'Failed to apply the Windows GPU patch' }
