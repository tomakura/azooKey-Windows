param(
    [ValidateSet('cpu','cuda','vulkan')][string]$Backend = 'cuda',
    [string]$Label = 'baseline',
    [string]$SwiftDll,
    [string]$ServerExe,
    [ValidateRange(1,8)][int]$InferenceLimit = 1,
    [ValidateSet('inference_latency','conversion_quality')][string]$Benchmark = 'inference_latency',
    [switch]$RequireGpuOffload
)
$ErrorActionPreference = 'Stop'
$repo = Split-Path $PSScriptRoot -Parent
$installed = Join-Path $env:APPDATA 'Azookey'
$root = Join-Path $repo ('target/verification/inference-' + $Label + '-' + $Backend)
$engine = Join-Path $root 'engine'
$profile = Join-Path $root 'profile'
New-Item -ItemType Directory -Path $engine,(Join-Path $profile 'Azookey') -Force | Out-Null
Copy-Item -Path (Join-Path $installed '*.dll') -Destination $engine -Force
$serverSource = if ($ServerExe) { $ServerExe } else { Join-Path $installed 'azookey-server.exe' }
Copy-Item -LiteralPath $serverSource -Destination (Join-Path $engine 'azookey-server.exe') -Force
if ($SwiftDll) { Copy-Item -LiteralPath $SwiftDll -Destination (Join-Path $engine 'azookey-server.dll') -Force }
foreach ($directory in (Get-ChildItem -LiteralPath $installed -Directory | Where-Object { $_.Name -in @('Dictionary','EmojiDictionary') -or $_.Name -like '*.resources' })) {
    $link = Join-Path $engine $directory.Name
    if (!(Test-Path -LiteralPath $link)) { New-Item -ItemType Junction -Path $link -Target $directory.FullName | Out-Null }
}
$model = Join-Path $engine 'zenz.gguf'
if (!(Test-Path -LiteralPath $model)) { New-Item -ItemType HardLink -Path $model -Target (Join-Path $installed 'zenz.gguf') | Out-Null }
$config = Get-Content -LiteralPath (Join-Path $installed 'settings.json') -Raw | ConvertFrom-Json
$config.zenzai.enable = $true
$config.zenzai.backend = $Backend
$config.zenzai.inference_limit = $InferenceLimit
$config.zenzai.model_path = ''
$config.conversion.live_conversion = $false
$config.learning.enable = $false
$config | ConvertTo-Json -Depth 20 | Set-Content -LiteralPath (Join-Path $profile 'Azookey/settings.json') -Encoding utf8
$originalAppdata = $env:APPDATA
$originalPath = $env:PATH
$originalInstance = $env:AZOOKEY_INSTANCE
$server = $null
try {
    $env:APPDATA = $profile
    $env:AZOOKEY_INSTANCE = 'inference-' + $Label + '-' + $Backend
    $env:PATH = (Join-Path $installed ('llama_' + $Backend)) + ';' + $originalPath
    $log = Join-Path $root 'server.log'
    $server = Start-Process -FilePath (Join-Path $engine 'azookey-server.exe') -WindowStyle Hidden -PassThru -RedirectStandardOutput $log -RedirectStandardError (Join-Path $root 'server-error.log')
    $ready = $false
    for ($attempt = 0; $attempt -lt 60; $attempt++) {
        Start-Sleep -Milliseconds 500
        $server.Refresh()
        if ($server.HasExited) { throw 'Test server exited' }
        if ((Get-Content -LiteralPath $log -Raw) -match 'listening') { $ready = $true; break }
    }
    if (!$ready) { throw 'Test server failed to start' }
    & (Join-Path $repo ('target/debug/examples/' + $Benchmark + '.exe')) > (Join-Path $root 'results.json')
    if ($LASTEXITCODE -ne 0) { throw 'Inference benchmark failed' }
    if ($RequireGpuOffload) {
        $errorLog = Get-Content -LiteralPath (Join-Path $root 'server-error.log') -Raw
        if ($errorLog -notmatch 'offloaded (\d+)/(\d+) layers to GPU' -or
            [int]$Matches[1] -le 0 -or $Matches[1] -ne $Matches[2]) {
            throw 'The model was not fully offloaded to the GPU'
        }
    }
} finally {
    if ($server -and !$server.HasExited) { Stop-Process -Id $server.Id -Force; $server.WaitForExit() }
    $env:APPDATA = $originalAppdata
    $env:PATH = $originalPath
    $env:AZOOKEY_INSTANCE = $originalInstance
}
Get-Content -LiteralPath (Join-Path $root 'results.json')
