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
| Android 8+ (arm64) | **Receives** | `zenshare-<ver>-android.apk` | Launches, discovers peers, receives files. Sending needs SAF — see below. |
| iOS | **Not yet** | — | Same work as Android, plus Apple's sandbox forbids the multicast trick. |
| Browser | **Impossible as designed** | — | See below. This one is not a scheduling problem. |

Every asset ships with `SHA256SUMS.txt` in the same release.

## Android — what shipped, and what is left

The APK exists, builds in CI on every push, and is attached to every release.

It is a `cdylib` loaded into `android.app.NativeActivity` — no Gradle, no Android
Gradle Plugin, no Java, no Kotlin, no dex. The APK is assembled with `aapt2`,
`zipalign` and `apksigner`, which ship with the SDK. `android:hasCode="false"` is
literally true: there is one shared library and one manifest.

What works today:

- Launches as a normal app called ZenShare
- Discovers peers on the same Wi-Fi, which required a `WifiManager.MulticastLock`
  acquired over JNI — Android's Wi-Fi stack drops multicast packets without one,
  and discovery fails silently rather than reporting an error
- Receives files into `Android/data/dev.zenshare.app/files/ZenShare`, visible
  over USB and in file managers, with no storage permission required

What does not:

- **Sending.** Picking a file needs the Storage Access Framework: an
  `ACTION_OPEN_DOCUMENT` intent, an Activity result callback, and a content-URI
  reader in place of a filesystem path. That is the next piece of work, and the
  UI currently says "sending from this phone is not wired up yet" rather than
  offering a button that does nothing.
- **Layout.** The interface is still laid out for a 1080x700 desktop window. On a
  phone it is cramped. Responsive layout is the task after SAF.
- **iOS.** Same port, plus a sandbox that forbids the multicast approach.

32-bit ARM is not built. Every phone sold since roughly 2018 is arm64, and the
extra ABI would double build time for devices nobody is testing on.

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
              ├── android apk       ubuntu-latest + NDK via cargo-ndk
              └── publish: SHA256SUMS.txt, draft → latest
```

Nothing is published by hand. The only manual step is writing the tag.
