$appDir = "$env:APPDATA\Azookey"

if (-not (Test-Path $appDir)) {
    Write-Error "Azookey not installed at $appDir — run the installer first"
    exit 1
}

$dllDest = "$appDir\azookey.dll"

# Kill fixed Azookey processes first
Write-Host "Stopping Azookey processes..."
@("ctfmon", "launcher", "azookey-server", "ui", "llama-server") | ForEach-Object {
    taskkill /F /IM "$_.exe" 2>$null
}
# プロセスが完全に終了するまで待つ
$waited = 0
while ((Get-Process -Name azookey-server, ui -ErrorAction SilentlyContinue) -and $waited -lt 5000) {
    Start-Sleep -Milliseconds 100
    $waited += 100
}

# Try to copy — if it fails, show which process is holding the DLL
Write-Host "Copying build artifacts..."
try {
    Copy-Item "target\debug\azookey_windows.dll" $dllDest -Force -ErrorAction Stop
    Write-Host "  azookey.dll updated"
} catch {
    Write-Host "  azookey.dll is locked (sign out/in to apply DLL changes)" -ForegroundColor Yellow
}

Copy-Item "target\debug\ui.exe"              "$appDir\" -Force
Copy-Item "target\debug\azookey-server.exe"  "$appDir\" -Force
Copy-Item "target\debug\launcher.exe"        "$appDir\" -Force

# Swift converter DLL + runtime (required by azookey-server.exe)
Copy-Item "server-swift\.build\x86_64-unknown-windows-msvc\release\azookey-server.dll" "$appDir\" -Force
$swiftRuntimes = Join-Path $env:LOCALAPPDATA "Programs\Swift\Runtimes"
if (Test-Path $swiftRuntimes) {
    $latestRuntime = Get-ChildItem $swiftRuntimes -Directory | Sort-Object Name | Select-Object -Last 1
    Copy-Item (Join-Path $latestRuntime.FullName "usr\bin\*") "$appDir\" -Force
}
if (-not (Test-Path "$appDir\Dictionary")) {
    Copy-Item "server-swift\azooKey_dictionary_storage\Dictionary" "$appDir\" -Recurse -Force
    Write-Host "Copied Dictionary"
}
if (-not (Test-Path "$appDir\EmojiDictionary")) {
    Copy-Item "server-swift\azooKey_emoji_dictionary_storage\EmojiDictionary" "$appDir\" -Recurse -Force
    Write-Host "Copied EmojiDictionary"
}

if (Test-Path "zenz.gguf")      { Copy-Item "zenz.gguf" "$appDir\" -Force; Write-Host "Copied zenz.gguf" }
if (Test-Path "llama_vulkan")   { Copy-Item "llama_vulkan" "$appDir\" -Recurse -Force; Write-Host "Copied llama_vulkan" }
if (Test-Path "llama_cuda")     { Copy-Item "llama_cuda"   "$appDir\" -Recurse -Force; Write-Host "Copied llama_cuda" }
if (Test-Path "llama_cpu")      { Copy-Item "llama_cpu"    "$appDir\" -Recurse -Force; Write-Host "Copied llama_cpu" }

Write-Host "Granting AppContainer permissions..."
icacls $dllDest /grant "*S-1-15-2-1:(RX)" | Out-Null

Write-Host "Starting TSF (ctfmon.exe)..."
Start-Process "$env:windir\system32\ctfmon.exe"
Start-Sleep -Milliseconds 300

Write-Host "Starting Azookey (requires admin for ui.exe)..."
Start-Process "$appDir\launcher.exe" -Verb RunAs
Write-Host "Done! (focus a text field and try typing)"
