param([string]$PackagePath = (Join-Path $PSScriptRoot '../build/release'))
$ErrorActionPreference = 'Stop'
$package = (Resolve-Path -LiteralPath $PackagePath).Path
$manifest = Get-Content -LiteralPath (Join-Path $package 'manifest.json') -Raw | ConvertFrom-Json
foreach ($item in $manifest) {
    $path = Join-Path $package $item.path
    if ((Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash -ne $item.sha256) { throw "Hash mismatch: $($item.path)" }
    if ((Get-Item -LiteralPath $path).Length -ne $item.bytes) { throw "Size mismatch: $($item.path)" }
}
$files = @(Get-ChildItem -LiteralPath $package -Recurse -File | Where-Object Name -ne 'manifest.json')
if ($files.Count -ne @($manifest).Count) { throw 'Manifest does not cover all package files' }
function Assert-Machine([string]$RelativePath, [uint16]$Expected) {
    $stream = [IO.File]::OpenRead((Join-Path $package $RelativePath))
    $reader = [IO.BinaryReader]::new($stream)
    try {
        if ($reader.ReadUInt16() -ne 0x5a4d) { throw "Not a PE binary: $RelativePath" }
        $stream.Position = 0x3c
        $offset = $reader.ReadInt32()
        $stream.Position = $offset
        if ($reader.ReadUInt32() -ne 0x4550 -or $reader.ReadUInt16() -ne $Expected) { throw "Wrong architecture: $RelativePath" }
    } finally { $reader.Dispose(); $stream.Dispose() }
}
foreach ($file in @('azookey-server.exe','azookey_settings.exe','launcher.exe','ui.exe','azookey_windows.dll','azookey-server.dll','llama_cpu/llama.dll','llama_cuda/llama.dll','llama_vulkan/llama.dll')) { Assert-Machine $file 0x8664 }
Assert-Machine 'x86/azookey_windows.dll' 0x14c
foreach ($resource in @('Dictionary','EmojiDictionary','swiftCore.dll','zenz.gguf')) {
    if (!(Test-Path -LiteralPath (Join-Path $package $resource))) { throw "Missing resource: $resource" }
}
if (!(Get-ChildItem -LiteralPath $package -Directory -Filter '*.resources')) { throw 'Missing Swift resource bundles' }
Write-Output "Verified $($files.Count) package files, SHA256 hashes, resources, and x64/x86 architectures."
