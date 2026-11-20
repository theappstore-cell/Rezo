param(
  [string]$Exe = "$env:ProgramFiles\Rezo\Rezo.exe",
  [int]$Port = 49494
)

if (-not ([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
  Write-Error "Run this script as administrator."; exit 1
}

Get-NetFirewallRule -DisplayName 'Rezo*' -ErrorAction SilentlyContinue | Remove-NetFirewallRule

New-NetFirewallRule -DisplayName 'Rezo HTTP (Sonos/UPnP/Web)' -Direction Inbound -Protocol TCP -LocalPort $Port `
  -Action Allow -Profile Private | Out-Null

New-NetFirewallRule -DisplayName 'Rezo UDP (SSDP/mDNS)' -Direction Inbound -Protocol UDP `
  -Program $Exe -Action Allow -Profile Private | Out-Null

Write-Host "Rules created: TCP $Port + UDP for $Exe (Private profile)."
Write-Host "If your network is classified as Public, switch it to Private: Settings > Network > Properties."
