# Exécuté par l'installateur (administrateur) : approuve le certificat de DCleaner et enregistre le menu Win11.
# Un échec ici n'empêche pas l'installation de l'app : le menu est simplement absent.
param([Parameter(Mandatory)][string]$Dir)
$ErrorActionPreference = 'Stop'
$Dir = $Dir.TrimEnd('\')
try {
    # Le certificat n'est approuvé que pour les paquets signés avec sa clé privée (conservée par l'éditeur).
    Import-Certificate -FilePath "$Dir\DCleaner.cer" -CertStoreLocation Cert:\LocalMachine\TrustedPeople | Out-Null
    Get-AppxPackage -Name DCleaner | Remove-AppxPackage
    Add-AppxPackage -Path "$Dir\DCleaner.msix" -ExternalLocation $Dir
} catch {
    Write-Host "Menu contextuel non enregistré : $_"
    exit 0
}
