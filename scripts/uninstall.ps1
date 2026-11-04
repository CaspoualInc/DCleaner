# Retire l'entrée du menu contextuel Windows 11 (et l'identité du paquet).
Get-AppxPackage -Name DCleaner | Remove-AppxPackage
Write-Host "DCleaner retiré du menu contextuel."
