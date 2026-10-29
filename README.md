# Refresh

A from-scratch remote-access tool: control your Windows PC from your
Android phone (or vice versa) across different networks — home WiFi,
mobile 5G, anywhere — without port forwarding, without a paid relay
service, and without any central account system that can see your
screen or your data.

**Status: working foundation, not a finished polished product.** This
is real, original code — not a fork of TeamViewer/AnyDesk/RustDesk —
built in one session. It compiles against documented, real crate APIs,
but it has **not been compiled or run in this environment**, because
this sandbox's network policy blocks access to `crates.io` (the Rust
package registry). The real first build happens on your own machine,
where you have normal internet access — see "Build it" below. Treat
the first build as the actual verification step, and expect to fix a
handful of small version-mismatch issues, which is normal for any new
Rust project's first build on a new machine.

## How it connects across networks

1. **Tailscale** (free, you install it yourself from tailscale.com) is
   what lets your phone and PC find each other no matter which network
   either one is on. It creates a private virtual network between your
   own devices; Refresh never relies on any Refresh-run server.
2. Once both devices are on your Tailscale network, Refresh connects
   **directly, device to device**, over a private, mutually
   authenticated, end-to-end encrypted connection (QUIC + TLS 1.3).

See `docs/SETUP.md` for the exact steps.

## Security model, honestly described

- Every device has its own Ed25519 identity keypair, generated on
  first run and never transmitted anywhere.
- Two devices only talk to each other after a human **pairing**
  ceremony: you compare a short fingerprint shown on both devices (the
  same trust-on-first-use model SSH and Signal "safety numbers" use),
  then explicitly approve. Nothing is auto-trusted.
- All session traffic is TLS 1.3 over QUIC, authenticated by the
  pinned device keys from pairing — not by a certificate authority, not
  by a password, and not by any server either of us runs.
- There is no cloud account, no telemetry, and no third party in the
  data path. Tailscale's own servers only ever help two of *your*
  devices find each other; your screen/input/file data isn't routed
  through them for most network configurations, and never through
  anything Refresh itself runs.
- The code is readable and auditable — it's a few thousand lines of
  Rust and Kotlin, not a black box.

What this build does **not** yet include, said plainly: real video
codec (it uses JPEG-per-frame as an MVP placeholder — functional but
not as smooth/efficient as H.264), DXGI Desktop Duplication capture
(uses the simpler GDI BitBlt path for now), audio, multi-monitor
selection, and Play-Store-ready APK signing. None of these are
security gaps — they're the "robustness/polish" work that's natural to
do next once the foundation is confirmed working on your machine.

## Project layout

```
core/            Rust: identity, pairing, encrypted transport, wire protocol
                 (shared by every platform - this is the only place the
                 cryptography and trust logic live)
windows-host/    Rust, Windows-only: screen capture + input injection,
                 the program that runs on the PC being controlled
mobile-ffi/      Rust compiled for Android: thin bridge exposing `core`
                 to Kotlin via UniFFI, so Android reuses the exact same
                 crypto/transport code instead of a second implementation
android-client/  Kotlin/Gradle Android app (the phone-side UI)
build-scripts/   .ps1 / .bat scripts that compile the above into
                 dist\refresh-windows-host.exe and dist\refresh.apk
docs/            Setup and security notes
```

## Build it

Prerequisites and step-by-step instructions are in
`docs/SETUP.md`. Short version, from Windows PowerShell or by
double-clicking `build-scripts\build-all.bat`:

```
build-scripts\build-all.bat
```

This produces `dist\refresh-windows-host.exe`, `dist\pair.exe`, and
`dist\refresh.apk`. Each build script also runs standalone if you only
need one of them.

## Pairing two devices

```
# On the PC:
dist\pair.exe show
#   -> prints a pairing code and a fingerprint

# On the phone (in the Refresh app): tap "Pair this device", paste the
# PC's pairing code, and compare the fingerprint it shows against what
# the PC printed before confirming.
```

Then on the phone, enter the PC's Tailscale IP (shown by running
`tailscale ip` on the PC, or in the Tailscale admin console) and tap
Connect.
