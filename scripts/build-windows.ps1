$ErrorActionPreference = "Stop"

$root = Split-Path -Parent $PSScriptRoot
$outDir = Join-Path $root "deploay\windows"
$linuxDir = Join-Path $root "deploay\linux"

New-Item -ItemType Directory -Force -Path $outDir | Out-Null
New-Item -ItemType Directory -Force -Path $linuxDir | Out-Null

$cargoBin = Join-Path $env:USERPROFILE ".cargo\bin"
if (Test-Path $cargoBin) {
    $env:PATH = "$cargoBin;$env:PATH"
}

Write-Host "Building Seller App (desktop-application-host)..." -ForegroundColor Cyan
Push-Location $root
npm run tauri build -- --bundles nsis
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
Pop-Location

Write-Host "Building Settings App (desktop-host-settings)..." -ForegroundColor Cyan
Push-Location (Join-Path $root "settings-app")
npm run tauri build -- --bundles nsis
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
Pop-Location

Copy-Item -Force (Join-Path $root "src-tauri\target\release\desktop-application-host.exe") $outDir
Copy-Item -Force (Join-Path $root "src-tauri\target\release\bundle\nsis\Desktop Application Host_*_x64-setup.exe") $outDir
Copy-Item -Force (Join-Path $root "settings-app\src-tauri\target\release\desktop-host-settings.exe") $outDir
Copy-Item -Force (Join-Path $root "settings-app\src-tauri\target\release\bundle\nsis\Desktop Host Settings_*_x64-setup.exe") $outDir

Write-Host "Windows artifacts deployed to: $outDir" -ForegroundColor Green
Get-ChildItem $outDir | Select-Object Name, Length
