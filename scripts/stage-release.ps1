param([ValidateSet('release', 'debug')][string]$Configuration = 'release')
$ErrorActionPreference = 'Stop'
$repo = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$output = Join-Path $repo 'build/release'
if (Test-Path -LiteralPath $output) {
    # Preserve previous packages; never mix old dictionaries or runtimes into a new build.
    $backup = Join-Path $repo ('target/verification/package-' + (Get-Date -Format 'yyyyMMdd-HHmmss-fff'))
    if (!$output.StartsWith($repo + [IO.Path]::DirectorySeparatorChar) -or !$backup.StartsWith($repo + [IO.Path]::DirectorySeparatorChar)) {
        throw 'Package paths must remain inside the repository'
    }
    New-Item -ItemType Directory -Path (Split-Path $backup) -Force | Out-Null
    Move-Item -LiteralPath $output -Destination $backup
}
New-Item -ItemType Directory -Path (Join-Path $output 'x86') -Force | Out-Null
foreach ($name in @('azookey-server.exe', 'ui.exe', 'launcher.exe', 'azookey_windows.dll')) {
    Copy-Item -LiteralPath (Join-Path $repo "target/$Configuration/$name") -Destination $output
}
Copy-Item -LiteralPath (Join-Path $repo "target/$Configuration/frontend.exe") -Destination (Join-Path $output 'azookey_settings.exe')
Copy-Item -LiteralPath (Join-Path $repo "target/i686-pc-windows-msvc/$Configuration/azookey_windows.dll") -Destination (Join-Path $output 'x86')
$swiftBuild = Join-Path $repo "server-swift/.build/x86_64-unknown-windows-msvc/$Configuration"
# cargo builds link against the release Swift engine, including during debug Rust builds.
if ($Configuration -eq 'debug') { $swiftBuild = Join-Path $repo 'server-swift/.build/x86_64-unknown-windows-msvc/release' }
Copy-Item -LiteralPath (Join-Path $swiftBuild 'azookey-server.dll') -Destination $output
Get-ChildItem -LiteralPath $swiftBuild -Directory -Filter '*.resources' | ForEach-Object {
    Copy-Item -LiteralPath $_.FullName -Destination $output -Recurse
}
$runtime = Get-ChildItem -LiteralPath (Join-Path $env:LOCALAPPDATA 'Programs/Swift/Runtimes') -Directory |
    Sort-Object { [version]$_.Name } | Select-Object -Last 1
if (!$runtime) { throw 'Swift runtime is not installed' }
Copy-Item -Path (Join-Path $runtime.FullName 'usr/bin/*.dll') -Destination $output
foreach ($backend in @('cpu', 'cuda', 'vulkan')) {
    $source = Join-Path $repo "llama_$backend"
    if (!(Test-Path -LiteralPath (Join-Path $source 'llama.dll'))) { throw "Missing backend: $source" }
    $destination = Join-Path $output "llama_$backend"
    New-Item -ItemType Directory -Path $destination | Out-Null
    Copy-Item -Path (Join-Path $source '*.dll') -Destination $destination
}
Copy-Item -LiteralPath (Join-Path $repo 'server-swift/azooKey_dictionary_storage/Dictionary') -Destination $output -Recurse
Copy-Item -LiteralPath (Join-Path $repo 'server-swift/azooKey_emoji_dictionary_storage/EmojiDictionary') -Destination $output -Recurse
Copy-Item -LiteralPath (Join-Path $repo 'zenz.gguf') -Destination $output
$licenses = Join-Path $output 'licenses'
New-Item -ItemType Directory -Path $licenses | Out-Null
Copy-Item -LiteralPath (Join-Path $repo 'LICENSE') -Destination (Join-Path $licenses 'azookey-windows.txt')
Copy-Item -LiteralPath (Join-Path $repo 'server-swift/Sources/azookey-server/Resources/SCOWL-LICENSE.txt') -Destination (Join-Path $licenses 'scowl.txt')
Copy-Item -LiteralPath (Join-Path $repo 'server-swift/azooKey_dictionary_storage/LICENSE') -Destination (Join-Path $licenses 'dictionary.txt')
Copy-Item -LiteralPath (Join-Path $repo 'server-swift/.build/checkouts/AzooKeyKanaKanjiConverter/LICENSE') -Destination (Join-Path $licenses 'converter.txt')
Get-ChildItem -LiteralPath $output -File -Recurse | ForEach-Object {
    [pscustomobject]@{ path = $_.FullName.Substring($output.Length + 1); sha256 = (Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash; bytes = $_.Length }
} | ConvertTo-Json -Depth 3 | Set-Content -LiteralPath (Join-Path $output 'manifest.json') -Encoding utf8
Write-Output "Release staged: $output"
