param([switch]$Run, [switch]$Test, [string]$OutputDirectory = 'artifacts/SlimBrowser-Rust-network')
$ErrorActionPreference = 'Stop'
Set-Location $PSScriptRoot
$localCargo = Join-Path $PSScriptRoot '.tools/rust/cargo/bin/cargo.exe'
if (Test-Path -LiteralPath $localCargo) {
    $env:CARGO_HOME = Join-Path $PSScriptRoot '.tools/rust/cargo'
    $env:RUSTUP_HOME = Join-Path $PSScriptRoot '.tools/rust/rustup'
    $env:PATH = (Join-Path $env:CARGO_HOME 'bin') + ';' + $env:PATH
    $cargo = $localCargo
} else { $cargo = 'cargo' }
Push-Location (Join-Path $PSScriptRoot 'rust')
try {
    if ($Test) {
        & $cargo test --locked
        if ($LASTEXITCODE -ne 0) { throw 'Rust unit tests failed.' }
    }
    & $cargo build --release --locked
    if ($LASTEXITCODE -ne 0) { throw 'Rust build failed.' }
} finally { Pop-Location }
$output = Join-Path $PSScriptRoot $OutputDirectory
New-Item -ItemType Directory -Path $output -Force | Out-Null
$exe = Join-Path $output 'SlimBrowser-Rust.exe'
Copy-Item -LiteralPath (Join-Path $PSScriptRoot 'rust/target/release/slimbrowser-rust.exe') -Destination $exe -Force
Copy-Item -LiteralPath (Join-Path $PSScriptRoot 'rust/README.md') -Destination (Join-Path $output 'README.md') -Force
if ($Test) {
    $process = Start-Process -FilePath $exe -ArgumentList '--smoke-test' -PassThru -Wait -WindowStyle Hidden
    Get-Content -LiteralPath (Join-Path $output 'smoke-test.txt')
    if ($process.ExitCode -ne 0) { throw 'Rust WebView2 smoke-test failed.' }
}
Write-Output "Rust build: $exe"
if ($Run) { Start-Process -FilePath $exe }
