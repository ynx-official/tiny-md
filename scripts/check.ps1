$ErrorActionPreference = 'Stop'
Push-Location (Join-Path $PSScriptRoot '..')
try {
    . (Join-Path $PSScriptRoot 'setup-windows.ps1')
    cargo fmt --all -- --check
    if ($LASTEXITCODE -ne 0) { throw 'Formatting check failed' }
    cargo test --locked --workspace
    if ($LASTEXITCODE -ne 0) { throw 'Tests failed' }
    cargo clippy --locked --workspace --all-targets -- -D warnings
    if ($LASTEXITCODE -ne 0) { throw 'Clippy check failed' }
} finally {
    Pop-Location
}
