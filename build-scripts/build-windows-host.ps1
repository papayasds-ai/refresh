#Requires -Version 5.1
<#
.SYNOPSIS
  Builds the Refresh Windows host (refresh-windows-host.exe) and the
  `pair` pairing CLI from source, on this machine.

.DESCRIPTION
  This does a real compile from the Rust source in this repo - it does
  not download a pre-built binary from anywhere. You need the Rust
  toolchain installed first (see PREREQUISITES below). That's the
  trade-off for not shipping a binary blob you'd have to blindly trust.

.PREREQUISITES
  - Rust + Cargo: https://rustup.rs  (choose the MSVC toolchain when asked)
  - The "Desktop development with C++" workload from Visual Studio
    Build Tools (needed by the MSVC linker Rust uses on Windows):
    https://visualstudio.microsoft.com/visual-cpp-build-tools/

.OUTPUT
  dist\refresh-windows-host.exe
  dist\pair.exe
#>

$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
Set-Location $root

# If this script was double-clicked (or run via "Run with PowerShell")
# rather than launched from build-all.bat, the window would otherwise
# vanish the instant the script ends - so we always pause before
# closing, whether it succeeded or failed.
$pauseAtEnd = $true

function Fail($message) {
    Write-Host ""
    Write-Host "ERROR: $message" -ForegroundColor Red
    if ($pauseAtEnd) {
        Write-Host ""
        Read-Host "Press Enter to close this window"
    }
    exit 1
}

function Require-Command($name, $hint) {
    if (-not (Get-Command $name -ErrorAction SilentlyContinue)) {
        Fail "'$name' was not found on PATH.`n$hint"
    }
}

try {
    Write-Host "== Refresh: building Windows host ==" -ForegroundColor Cyan
    Require-Command "cargo" "Install Rust from https://rustup.rs, then close and reopen this terminal/explorer so PATH updates, then try again."

    Write-Host "Compiling refresh-core, refresh-windows-host, and the pair CLI (release mode)..."
    cargo build --release -p refresh-windows-host -p refresh-core --bin pair --bin refresh-windows-host
    if ($LASTEXITCODE -ne 0) {
        Fail "Build failed. See the compiler errors printed above - most often this means a dependency needs a small version adjustment for your exact toolchain. Paste the exact error back if you want help fixing it."
    }

    New-Item -ItemType Directory -Force -Path "dist" | Out-Null
    Copy-Item "target\release\refresh-windows-host.exe" "dist\refresh-windows-host.exe" -Force
    Copy-Item "target\release\pair.exe" "dist\pair.exe" -Force

    Write-Host "`nBuilt:" -ForegroundColor Green
    Write-Host "  dist\refresh-windows-host.exe  - run this on the PC you want to reach remotely"
    Write-Host "  dist\pair.exe                  - run 'dist\pair.exe show' to get this PC's pairing code"

    Write-Host "`nNote on code signing:" -ForegroundColor Yellow
    Write-Host "These .exe files are NOT signed with a code-signing certificate."
    Write-Host "Windows SmartScreen may warn on first run of an unsigned binary you built yourself - that is expected, not a sign of a problem with the build."
    Write-Host "If you own a code-signing certificate, sign it yourself with:"
    Write-Host "  signtool sign /f yourcert.pfx /p yourpassword /t http://timestamp.digicert.com dist\refresh-windows-host.exe"
}
catch {
    Fail "Unexpected error: $($_.Exception.Message)"
}

if ($pauseAtEnd) {
    Write-Host ""
    Read-Host "Done. Press Enter to close this window"
}
