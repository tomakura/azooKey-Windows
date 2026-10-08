param()

$ErrorActionPreference = "Stop"

$repo = Resolve-Path (Join-Path $PSScriptRoot "..")
$installerPath = Join-Path $repo "installer\Installer.iss"
$makefilePath = Join-Path $repo "Makefile.toml"

$installer = Get-Content $installerPath -Raw
$makefile = Get-Content $makefilePath -Raw

$requiredInstallerFragments = @(
    'Source: "../build/release/azookey_windows.dll"',
    'Source: "../build/release/x86/azookey_windows.dll"',
    'Source: "../build/release/*"',
    'Source: "./Azookey Startup.xml"',
    'LoadStringFromFile(TaskXmlPath, TaskXmlContentAnsi);'
)

foreach ($fragment in $requiredInstallerFragments) {
    if (-not $installer.Contains($fragment)) {
        throw "Installer.iss is missing required fragment: $fragment"
    }
}

$requiredMakefileFragments = @(
    'scripts/stage-release.ps1',
    'node $npmCli run tauri build'
)

foreach ($fragment in $requiredMakefileFragments) {
    if (-not $makefile.Contains($fragment)) {
        throw "Makefile.toml is missing required fragment: $fragment"
    }
}

Write-Host "Installer static verification passed."

$staging = Get-Content (Join-Path $repo "scripts/stage-release.ps1") -Raw
foreach ($fragment in @('azookey-server.dll', '*.resources', 'Programs/Swift/Runtimes', 'Dictionary', 'EmojiDictionary', 'zenz.gguf', 'manifest.json')) {
    if (!$staging.Contains($fragment)) { throw "Package staging is missing: $fragment" }
}
