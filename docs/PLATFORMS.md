# Platform support

What you can download today, what is missing, and what it would actually take.
Written down because "coming soon" without a reason is how projects lie to their
users.

## Today

| Platform | Status | Download | Notes |
|---|---|---|---|
| macOS 11+ | **Works** | `zenshare-<ver>-macos-universal.zip` | One universal binary, Apple Silicon and Intel. Unsigned: right-click → Open on first launch. |
| Windows 10/11 x64 | **Works** | `zenshare-<ver>-windows-x86_64.zip` | Unsigned: SmartScreen → More info → Run anyway. |
| Linux x86_64 | **Works** | `zenshare-<ver>-linux-x86_64.tar.gz` | Needs `libxkbcommon`, `libwayland`, `libGL` — present on any desktop distro. |
| Linux aarch64 | **Works** | `zenshare-<ver>-linux-aarch64.tar.gz` | Raspberry Pi 4/5 and other ARM64 boards. |
| Android | **Not yet** | — | See below. This is the biggest gap. |
| iOS | **Not yet** | — | Same work as Android, plus Apple's sandbox forbids the multicast trick. |
| Browser | **Impossible as designed** | — | See below. This one is not a scheduling problem. |

Every asset ships with `SHA256SUMS.txt` in the same release.

## Android — a port, not a build flag

The app is a desktop program. Making it run on a phone is roughly two days of
real work, and CI cannot substitute for it:

1. **Entry point.** Android has no `main()`. eframe needs a `cdylib` with a
   `#[no_mangle] android_main` symbol, driven by `android-activity`.
2. **Packaging.** An `AndroidManifest.xml`, a Gradle project, and `cargo-ndk`
   producing `.so` files for `aarch64-linux-android`, `armv7`, and `x86_64`.
3. **Multicast.** Android drops multicast packets unless the app holds a
   `WifiManager.MulticastLock`. Without it, discovery silently finds nothing —
   the same failure mode as the sandbox this was developed in.
4. **Permissions.** `INTERNET`, `ACCESS_NETWORK_STATE`, `ACCESS_WIFI_STATE`, and
   on Android 13+ `NEARBY_WIFI_DEVICES`.
5. **Storage.** `~/Downloads` does not exist. The receiver needs a SAF
   (`ACTION_OPEN_DOCUMENT_TREE`) destination, which is a JNI bridge.

The release workflow is deliberately not shipping a stub APK. An app that
installs and then crashes on launch is worse than no app: it teaches the user
that the project is broken rather than incomplete.

## Browser — impossible without a relay

This is worth being precise about, because it is the question people ask first.

QUIC runs over **UDP**. A browser cannot open a raw UDP socket; that is a
deliberate security property of the platform, not a missing feature. WebTransport
exists, but it is a client for an HTTP/3 *server* — it cannot dial another
browser on your LAN.

So a web version has exactly two options:

- **A relay server** in the middle, which means an account, a cost, and someone
  else holding your files. That is the product this project exists to replace.
- **A different protocol** over WebRTC data channels, which needs a signalling
  server anyway.

`site/` is therefore a download page, not a web app. It is deployed to GitHub
Pages and exists so you can grab the build from your phone's browser and read
the honest limitations before installing anything.

## How a release is produced

```
git tag v0.1.0 && git push --tags
        │
        ├── CI (on every push)  fmt · clippy -D warnings · tests · cargo-deny
        │                       build + `doctor` smoke test on Linux, macOS, Windows
        │
        └── Release (on v* tag)
              ├── draft release with product notes
              ├── linux x86_64      ubuntu-latest
              ├── linux aarch64     ubuntu-24.04-arm
              ├── macos universal   macos-14, lipo + ad-hoc codesign
              ├── windows x86_64    windows-latest
              └── publish: SHA256SUMS.txt, draft → latest
```

Nothing is published by hand. The only manual step is writing the tag.
