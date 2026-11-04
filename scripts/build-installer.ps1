# Construit l'installateur NSIS : target\release\bundle\nsis\DCleaner_<version>_x64-setup.exe
# Prérequis : Rust, cargo-tauri (cargo install tauri-cli --version "^2"), Windows SDK (makeappx, signtool).
# Le paquet MSIX du menu Win11 est signé par un certificat auto-signé créé ici (magasin utilisateur, clé non exportée).
$ErrorActionPreference = 'Stop'
$root = Split-Path $PSScriptRoot
Set-Location $root

function Run($exe, [string[]]$a) { & $exe @a; if ($LASTEXITCODE) { throw "$exe a échoué ($LASTEXITCODE)" } }

Run cargo @('build', '--release', '-p', 'dcleaner_shell')  # la DLL est une ressource de l'installateur

$sdk = Get-ChildItem "${env:ProgramFiles(x86)}\Windows Kits\10\bin" -Directory | Where-Object Name -Match '^10\.' | Sort-Object Name | Select-Object -Last 1
if (-not $sdk) { throw 'Windows SDK introuvable (makeappx / signtool).' }
$bin = Join-Path $sdk.FullName 'x64'

# Doit être identique à Publisher dans package\AppxManifest.xml
$subject = 'CN=DCleaner'
$cert = Get-ChildItem Cert:\CurrentUser\My | Where-Object { $_.Subject -eq $subject -and $_.NotAfter -gt (Get-Date).AddDays(30) } | Select-Object -First 1
if (-not $cert) {
    $cert = New-SelfSignedCertificate -Type Custom -Subject $subject -KeyUsage DigitalSignature -FriendlyName 'DCleaner (auto-signé)' `
        -CertStoreLocation Cert:\CurrentUser\My -NotAfter (Get-Date).AddYears(5) `
        -TextExtension @('2.5.29.37={text}1.3.6.1.5.5.7.3.3', '2.5.29.19={text}')
}

$res = Join-Path $root 'installer-res'
$stage = Join-Path $res 'stage'
Remove-Item $res -Recurse -Force -ErrorAction SilentlyContinue
New-Item -ItemType Directory (Join-Path $stage 'Assets') -Force | Out-Null
Copy-Item (Join-Path $root 'package\AppxManifest.xml') $stage
Copy-Item (Join-Path $root 'package\Assets\*') (Join-Path $stage 'Assets')
Export-Certificate -Cert $cert -FilePath (Join-Path $res 'DCleaner.cer') | Out-Null

# /nv : paquet « sparse », l'exe et la DLL sont dans le dossier d'installation (ExternalLocation)
Run (Join-Path $bin 'makeappx.exe') @('pack', '/d', $stage, '/p', (Join-Path $res 'DCleaner.msix'), '/nv', '/o')
Run (Join-Path $bin 'signtool.exe') @('sign', '/fd', 'SHA256', '/sha1', $cert.Thumbprint, (Join-Path $res 'DCleaner.msix'))

Set-Location (Join-Path $root 'src-tauri')
Run cargo @('tauri', 'build', '--bundles', 'nsis', '--config', 'tauri.installer.conf.json')
Get-ChildItem (Join-Path $root 'target\release\bundle\nsis\*-setup.exe') | Select-Object Name, Length
