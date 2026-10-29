#Requires -Version 5.1
<#
.SYNOPSIS
  Builds the Refresh Android client (.apk) from source: compiles
  refresh-mobile-ffi (Rust) for each Android ABI, generates the Kotlin
  bindings to it, then builds the app with Gradle.

.PREREQUISITES
  - Rust + Cargo: https://rustup.rs
  - cargo-ndk:         cargo install cargo-ndk
  - Android NDK:       install via Android Studio > SDK Manager > SDK Tools
                        > NDK (side by side), or https://developer.android.com/ndk
  - Android SDK + a `gradlew.bat`: easiest path is to open the
    android-client/ folder once in Android Studio, which bootstraps the
    Gradle wrapper for you - then re-run this script.
  - Java 17 (Android Studio bundles one; JAVA_HOME should point at it)

.OUTPUT
  dist\refresh.apk
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
    Write-Host "== Refresh: building Android client ==" -ForegroundColor Cyan
    Require-Command "cargo" "Install Rust from https://rustup.rs"
    Require-Command "cargo-ndk" "Install it with: cargo install cargo-ndk"

    if (-not $env:ANDROID_NDK_HOME -and -not $env:NDK_HOME) {
        Fail "ANDROID_NDK_HOME (or NDK_HOME) is not set. Point it at your installed NDK directory, e.g.`n  `$env:ANDROID_NDK_HOME = 'C:\Users\you\AppData\Local\Android\Sdk\ndk\26.3.11579264'`nthen close and reopen this window and try again."
    }

    $jniLibs = "android-client\app\src\main\jniLibs"
    New-Item -ItemType Directory -Force -Path $jniLibs | Out-Null

    Write-Host "Cross-compiling refresh-mobile-ffi for Android ABIs (arm64-v8a, armeabi-v7a, x86_64)..."
    cargo ndk -t arm64-v8a -t armeabi-v7a -t x86_64 -o $jniLibs build --release -p refresh-mobile-ffi
    if ($LASTEXITCODE -ne 0) {
        Fail "Native build failed - see errors printed above. Paste the exact error back if you want help fixing it."
    }

    Write-Host "Generating Kotlin bindings from the Rust interface..."
    $kotlinOut = "android-client\app\src\main\java"
    cargo run --release -p refresh-mobile-ffi --bin uniffi-bindgen -- generate mobile-ffi\src\refresh.udl --language kotlin --out-dir $kotlinOut
    if ($LASTEXITCODE -ne 0) {
        Fail "Kotlin binding generation failed - see errors above."
    }

    $gradlew = "android-client\gradlew.bat"
    if (-not (Test-Path $gradlew)) {
        Fail "$gradlew not found.`nOpen the 'android-client' folder once in Android Studio (File > Open) - it will create the Gradle wrapper automatically on first sync - then re-run this script."
    }

    Write-Host "Building the APK with Gradle (debug variant - self-signed, directly installable on your own phone)..."
    Push-Location "android-client"
    try {
        & .\gradlew.bat assembleDebug
        if ($LASTEXITCODE -ne 0) {
            Fail "Gradle build failed - see errors above."
        }
    } finally {
        Pop-Location
    }

    New-Item -ItemType Directory -Force -Path "dist" | Out-Null
    $apk = Get-ChildItem "android-client\app\build\outputs\apk\debug\*.apk" | Select-Object -First 1
    if ($apk) {
        Copy-Item $apk.FullName "dist\refresh.apk" -Force
        Write-Host "`nBuilt: dist\refresh.apk" -ForegroundColor Green
    } else {
        Fail "Build finished but no .apk was found in the expected output folder."
    }

    Write-Host "`nNote on signing:" -ForegroundColor Yellow
    Write-Host "This APK is signed with Gradle's auto-generated debug key, which is fine"
    Write-Host "for installing on your own phone via 'Install unknown apps' but is NOT"
    Write-Host "suitable for publishing to the Play Store or sharing with others as-is."
    Write-Host "For that, add your own release signingConfig in android-client\app\build.gradle.kts"
    Write-Host "and build with '.\gradlew.bat assembleRelease' instead."
}
catch {
    Fail "Unexpected error: $($_.Exception.Message)"
}

if ($pauseAtEnd) {
    Write-Host ""
    Read-Host "Done. Press Enter to close this window"
}
