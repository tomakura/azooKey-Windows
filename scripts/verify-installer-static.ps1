param()

$ErrorActionPreference = "Stop"

$repo = Resolve-Path (Join-Path $PSScriptRoot "..")
$installerPath = Join-Path $repo "installer\Installer.iss"
$makefilePath = Join-Path $repo "Makefile.toml"

$installer = Get-Content $installerPath -Raw
$makefile = Get-Content $makefilePath -Raw

$requiredInstallerFragments = @(
    'Source: "../build/azookey_windows.dll"',
    'Source: "../build/x86/azookey_windows.dll"',
    'Source: "../build/*"',
    'Source: "../target/release/bundle/nsis/Azookey_0.1.0_x64-setup.exe"',
    'Source: "./Azookey Startup.xml"',
    'LoadStringFromFile(TaskXmlPath, TaskXmlContentAnsi);'
)

foreach ($fragment in $requiredInstallerFragments) {
    if (-not $installer.Contains($fragment)) {
        throw "Installer.iss is missing required fragment: $fragment"
    }
}

$requiredMakefileFragments = @(
    'cp target/$str/azookey-server.exe build',
    'cp target/$str/ui.exe build',
    'cp target/$str/launcher.exe build',
    'cp -Recurse -Force server-swift/azooKey_emoji_dictionary_storage/EmojiDictionary build',
    'cp -Recurse -Force server-swift/azooKey_dictionary_storage/Dictionary build',
    'node $npmCli run tauri build'
)

foreach ($fragment in $requiredMakefileFragments) {
    if (-not $makefile.Contains($fragment)) {
        throw "Makefile.toml is missing required fragment: $fragment"
    }
}

Write-Host "Installer static verification passed."
