# ZenShare {{VERSION}}

Send a file to the device next to you without an account, an upload, or a
"sign in to continue".

ZenShare talks straight to the other device over the network you are already on.
Everything is encrypted with TLS 1.3, every 4 MiB chunk is checked with BLAKE3
before it touches your disk, and nothing is stored anywhere in between. No ads,
no telemetry, no cloud. The only packets that leave your machine go to the
device you picked.

## Download

| Your machine | File |
| --- | --- |
| macOS (Apple Silicon or Intel) | [`zenshare-{{VERSION}}-macos-universal.zip`](https://github.com/{{REPO}}/releases/download/v{{VERSION}}/zenshare-{{VERSION}}-macos-universal.zip) |
| Windows 10 / 11 (64-bit) | [`zenshare-{{VERSION}}-windows-x86_64.zip`](https://github.com/{{REPO}}/releases/download/v{{VERSION}}/zenshare-{{VERSION}}-windows-x86_64.zip) |
| Linux (64-bit Intel / AMD) | [`zenshare-{{VERSION}}-linux-x86_64.tar.gz`](https://github.com/{{REPO}}/releases/download/v{{VERSION}}/zenshare-{{VERSION}}-linux-x86_64.tar.gz) |
| Linux (64-bit ARM, e.g. Raspberry Pi) | [`zenshare-{{VERSION}}-linux-aarch64.tar.gz`](https://github.com/{{REPO}}/releases/download/v{{VERSION}}/zenshare-{{VERSION}}-linux-aarch64.tar.gz) |

## First launch

These builds are not signed with a paid developer certificate, so both macOS and
Windows will warn you the first time. That is expected, and it is what the
warning is for: an app that nobody has vetted. Here is how to get past it.

**macOS** — unzip, then drag `ZenShare.app` to Applications. Right-click it and
choose **Open**, then **Open** again in the dialog. (Double-clicking will only
offer to delete it.) If macOS still refuses:

```sh
xattr -dr com.apple.quarantine /Applications/ZenShare.app
```

**Windows** — unzip anywhere and run `zenshare.exe`. SmartScreen will say
"Windows protected your PC"; click **More info → Run anyway**.

**Linux** — unpack and run:

```sh
tar -xzf zenshare-{{VERSION}}-linux-x86_64.tar.gz
cd zenshare-{{VERSION}}-linux-x86_64
./zenshare
```

You will need `libxkbcommon`, `libwayland`, and `libGL` present, which every
desktop distribution already ships.

## Using it

1. Open ZenShare on both devices, on the same Wi-Fi.
2. The other device appears under **Nearby** within a couple of seconds.
3. Drag files onto the window, pick the device, hit **Send**.

To check that the engine works on your machine without a second device:

```sh
./zenshare selftest --size-mb 512
```

It generates a payload, pushes it through a real QUIC connection, and compares
hashes. If that passes, the transfer path is sound.

## What is in this build

- Chunked transfer over QUIC with BBR, 32 streams in flight
- BLAKE3 verification on every chunk, before it is written
- Atomic landing: a transfer either arrives complete or leaves a `.part` file
- Drag-and-drop queue, custom-painted interface, no webview
- Peer discovery over the local network
- A headless sender: `zenshare send file.zip --to 192.168.1.24:47820`

## Known limits, stated plainly

- **No Android or iOS yet.** The desktop app is the part that works. A phone
  build needs an Android entry point, a Gradle wrapper, and a multicast lock —
  it is a port, not a build flag.
- **No browser version, and there cannot be one.** Browsers cannot open a raw
  UDP socket, and QUIC needs one. A web version would need a relay server in the
  middle, which is exactly the thing this project exists to avoid.
- **The pairing code is not enforced yet.** Connections are encrypted, but the
  six-digit confirmation screen is not wired up, so first contact with a stranger
  on a shared network is not yet verified.
- **Resume is plumbed but not active.** The bitmap travels on the wire; the
  receiver does not yet report which chunks it already has.
- **Discovery is unproven on networks that drop multicast.** Some corporate and
  guest Wi-Fi does. The app still runs; you just will not see peers.

## Verify your download

```sh
shasum -a 256 -c SHA256SUMS.txt
```

## Under the hood

83 direct crates, exactly one of which contains C (`ring`, for TLS). No webview,
no Electron, no browser engine. Built from
[source](https://github.com/{{REPO}}) with GitHub Actions on the tag you are
reading.
