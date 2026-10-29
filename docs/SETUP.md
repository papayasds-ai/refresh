# Setup

## 1. Install Tailscale on both devices

- PC: download from https://tailscale.com/download/windows, install,
  sign in (any method - Google/Microsoft/GitHub/email - this is just
  your own private network's login, nothing Refresh touches).
- Phone: install "Tailscale" from the Play Store, sign in with the
  same account.

Both devices now have a stable private IP like `100.x.y.z` that works
no matter which network either device is on. On the PC, running
`tailscale ip` in a terminal shows it.

## 2. Install the prerequisites for building Refresh

On the Windows PC you'll build from:

- **Rust**: https://rustup.rs — choose the default (MSVC) toolchain.
- **Visual Studio Build Tools**, "Desktop development with C++"
  workload: https://visualstudio.microsoft.com/visual-cpp-build-tools/
  (Rust needs its linker on Windows.)
- For the Android build specifically:
  - **Android Studio** (easiest way to get the Android SDK + NDK, and
    to bootstrap the Gradle wrapper): https://developer.android.com/studio
    After installing, open Android Studio once, go to
    **SDK Manager → SDK Tools**, and install **NDK (Side by side)**.
  - Open the `android-client` folder in Android Studio once (File →
    Open) and let it sync — this generates `gradlew.bat`, which the
    build script needs.
  - `cargo install cargo-ndk`
  - Set `ANDROID_NDK_HOME` to the installed NDK path, e.g.
    `setx ANDROID_NDK_HOME "C:\Users\you\AppData\Local\Android\Sdk\ndk\26.3.11579264"`
    (reopen your terminal after this)

## 3. Build

```
build-scripts\build-all.bat
```

or build just one side:

```
build-scripts\build-windows-host.ps1
build-scripts\build-android.ps1
```

Expect the **first** run to surface a few small issues — a dependency
needing `cargo update -p <crate>`, or an API detail that shifted
between crate versions. This is normal for freshly written Rust code
that hasn't been compiled yet in this exact environment; the fix is
almost always small (the compiler error tells you exactly which line
and crate). Ask Claude Code, or a Rust-focused assistant, to paste the
exact error if you get stuck — the codebase is small enough that this
goes quickly.

## 4. Run it

On the PC:
```
dist\refresh-windows-host.exe
```
Leave it running (a terminal window, for now — turning this into a
proper background Windows service/tray app is a natural next step once
you've confirmed everything works).

On the phone: install `dist\refresh.apk` (you'll need to allow
"install unknown apps" for whichever app you transfer it with, since
it isn't from the Play Store), open Refresh, pair, enter the PC's
Tailscale IP, and connect.

## 5. Firewall

Windows Firewall may prompt the first time `refresh-windows-host.exe`
listens on a port (UDP 7878) — allow it for **private networks**
(Tailscale's interface counts as one). You do not need to open
anything on your router; Tailscale handles reachability.
