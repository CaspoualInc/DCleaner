# Installe DCleaner et son entrée du menu contextuel Windows 11 (paquet sparse, mode développeur).
# Usage : powershell -ExecutionPolicy Bypass -File scripts\install.ps1 [-RestartExplorer]
param([switch]$RestartExplorer)
$ErrorActionPreference = 'Stop'
$root = Split-Path $PSScriptRoot
$rel = Join-Path $root 'target\release'

foreach ($f in 'dcleaner.exe', 'dcleaner_shell.dll') {
    if (-not (Test-Path (Join-Path $rel $f))) { throw "$f introuvable : lancez d'abord  cargo build --release" }
}
$dev = (Get-ItemProperty 'HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\AppModelUnlock' -ErrorAction SilentlyContinue).AllowDevelopmentWithoutDevLicense
if ($dev -ne 1) { throw "Activez le mode développeur : Paramètres > Système > Pour les développeurs." }

Get-AppxPackage -Name DCleaner | Remove-AppxPackage
if ($RestartExplorer) { Stop-Process -Name explorer -Force -ErrorAction SilentlyContinue; Start-Sleep 2 }

$dist = Join-Path $root 'dist'
New-Item -ItemType Directory -Force (Join-Path $dist 'Assets') | Out-Null
try {
    Copy-Item (Join-Path $rel 'dcleaner.exe'), (Join-Path $rel 'dcleaner_shell.dll') $dist -Force
} catch {
    throw "Fichiers verrouillés par l'Explorateur : relancez avec  -RestartExplorer"
}
Copy-Item (Join-Path $root 'package\Assets\*') (Join-Path $dist 'Assets') -Force
Copy-Item (Join-Path $root 'package\AppxManifest.xml') $dist -Force

Add-AppxPackage -Register (Join-Path $dist 'AppxManifest.xml') -ExternalLocation $dist
Write-Host "DCleaner installé. Clic droit sur un dossier ou un lecteur : « Inspecter et nettoyer avec DCleaner »."
Write-Host "Si l'entrée n'apparaît pas tout de suite : relancez avec -RestartExplorer (ferme brièvement vos fenêtres Explorateur)."
