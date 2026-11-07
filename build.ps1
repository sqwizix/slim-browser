param([switch]$Run)
$ErrorActionPreference = 'Stop'
Set-Location $PSScriptRoot
$localSdk = Join-Path $env:LOCALAPPDATA 'SlimBrowser-SDK/dotnet.exe'
$dotnet = if (Test-Path $localSdk) { $localSdk } else { 'dotnet' }
& $dotnet publish SlimBrowser.csproj -c Release -r win-x64 --self-contained false -o artifacts/SlimBrowser
if ($LASTEXITCODE -ne 0) { throw 'Build failed. Install .NET 10 SDK.' }
if ($Run) { Start-Process -FilePath (Join-Path $PSScriptRoot 'artifacts/SlimBrowser/SlimBrowser.exe') }
