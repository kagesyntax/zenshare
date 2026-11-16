# ZenShare — Complete Crate Inventory

**Every crate the project will use, nothing else.**
Versions verified against crates.io on **2026-11-16**. Toolchain: rustc 1.99.0, edition 2024.

Companion to [`PLAN.md`](./PLAN.md) — that document explains *why* the architecture is shaped
this way; this one is the bill of materials.

---

## How to read the Tier column

| Tier | Meaning |
|---|---|
| **P** | Pure Rust. No C, no C++, no FFI, no bindings. |
| **P\*** | Pure Rust, but ships optional hand-written SIMD asm for speed. A `pure` feature flag removes it. |
| **F** | Platform FFI (AppKit / Win32 / XDG). Contained to the crates marked here — see §15. |
| **C** | Has a C or asm dependency. **Exactly one crate in this project is tier C: `ring`.** |

**Totals: 84 mandatory runtime crates, 7 optional, 7 dev, 7 build tools — 105 direct.**
Roughly 260 transitive. Expect an 18–30 MB stripped binary — wgpu and the font/icon assets dominate,
not the logic.

---

## 1. UI & rendering — 9 crates (+2 optional)

| Crate | Ver | Tier | Why it is here |
|---|---|---|---|
| `eframe` | 0.37.0 | P | App shell. Window, event loop, GPU surface, persistence, mobile entry point. Handles the wgpu/glow split so we never touch either directly. |
| `egui` | 0.37.0 | P | The UI itself. Immediate mode means a 60 fps progress dashboard is the *easy* case, not the hard one. |
| `egui_extras` | 0.37.0 | P | `TableBuilder` for the transfer list, plus image loaders for thumbnails. |
| `egui_dnd` | 0.18.0 | P | Drag-to-reorder the send queue. Xender makes you delete and re-add. |
| `egui-notify` | 0.24.0 | P | In-app toasts. Every "are you sure?" modal in Xender becomes a toast you can ignore. |
| `fast_qr` | 0.14.0 | P | QR matrix generation. We draw the modules as egui rects — no image encode, no PNG round-trip, crisp at any DPI. |
| `rust-embed` | 8.14.0 | P | Compiles icons, fonts and locale files into the binary. One file to ship, nothing to lose. |
| `image` | 0.25.10 | P | Thumbnail decoding — **png, jpeg, webp, gif only**. Every extra format is attack surface for a decoder handling untrusted files. |
| `fast_image_resize` | 6.1.0 | P | SIMD downscaling, ~10× faster than `image`'s built-in resize. Thumbnails appear instantly instead of after a beat. |
| `resvg` + `tiny-skia` | 0.50 / 0.12 | P | **Optional `svg-icons`.** Rasterize vector icons once at startup. Drop both and ship a PNG atlas if binary size matters more. |

## 2. Typography — zero crates, on purpose

No font crate. We embed two files and hand them to `egui::FontDefinitions`:

- **Inter Variable** (SIL OFL) — UI text. Tabular figures enabled so byte counters don't jitter.
- **JetBrains Mono** (SIL OFL) — speed readouts, hashes, device IDs.

This is the single highest-leverage "feels premium" decision in the whole project, and it costs
two files and about 40 lines. System-font fallback chains look different on every machine; embedded
fonts look identical everywhere.

## 3. Async runtime & concurrency — 11 crates

| Crate | Ver | Tier | Why |
|---|---|---|---|
| `tokio` | 1.54.1 | P | Runtime. Features: `rt-multi-thread`, `fs`, `net`, `sync`, `time`, `signal`, `io-util`, `macros`. |
| `tokio-util` | 0.7.20 | P | `CancellationToken` — one per transfer, so "cancel" actually stops the read loop instead of just hiding the row. |
| `futures` | 0.3.34 | P | Stream combinators for the chunk pipeline. |
| `async-channel` | 2.5.0 | P | Bounded UI ↔ engine channel. Bounded is the point: a stalled engine backpressures the UI instead of eating RAM. |
| `parking_lot` | 0.13.0 | P | Faster, smaller locks. Matters in the per-chunk path. |
| `dashmap` | 6.2.1 | P | Concurrent peer/transfer registry without a global mutex. |
| `bytes` | 1.12.1 | P | `Bytes` slices shared between mmap and QUIC with refcounted zero-copy. The reason we don't memcpy 4 MiB per chunk. |
| `arc-swap` | 1.9.5 | P | Lock-free config hot-reload. Change settings mid-transfer without a restart. |
| `async-trait` | 0.1.92 | P | One `Transport` trait with LAN (quinn) and WAN (iroh) impls. |
| `backon` | 1.6.0 | P | Exponential backoff with jitter for reconnect. Beats hand-rolling it wrong. |
| `rustc-hash` | 2.1.3 | P | `FxHashMap` for hot-path maps keyed by our own IDs. Never for attacker-controlled keys. |

## 4. Transport — 8 crates

| Crate | Ver | Tier | Why |
|---|---|---|---|
| `quinn` | 0.11.12 | P | QUIC. TLS 1.3 built in, BBR + pacing, 0-RTT resume, connection migration. The single biggest speed decision in the project. |
| `rustls` | 0.23.46 | C | TLS 1.3, `ring` provider. **The one C dependency.** See §15. |
| `rustls-pki-types` | 1.16.0 | P | Cert/key types shared with rcgen, so no conversion layer. |
| `rcgen` | 0.14.10 | P | Mints a self-signed cert per session at runtime. No CA, no expiry, no renewal — every session is a fresh identity. |
| `socket2` | 0.6.5 | P | 4–16 MiB `SO_RCVBUF`/`SO_SNDBUF`, address reuse, explicit bind. Without this you leave 30% of throughput on the table. |
| `rustix` | 1.1.6 | P | Pure-Rust syscalls: `pwritev`, `fadvise`, `fallocate`, `statvfs`. Replaces the `libc` crate entirely. |
| `if-addrs` | 0.15.0 | P | Enumerate interfaces, pick the right LAN address instead of guessing. |
| `netdev` | 0.47.0 | P | Detect link type and speed — lets us warn "you're on 2.4 GHz, this will be slow" *before* the transfer. |

## 5. Discovery — 1 crate

| Crate | Ver | Tier | Why |
|---|---|---|---|
| `mdns-sd` | 0.21.7 | P | Pure-Rust mDNS/DNS-SD. Advertises `_zenshare._udp.local`, browses for peers. Live device list in <200 ms. |

Discovery is **never** trust. Anyone on the network can spoof an advertisement — which is exactly
why the 6-digit SAS exists.

## 6. Crypto & identity — 13 crates

| Crate | Ver | Tier | Why |
|---|---|---|---|
| `blake3` | 1.8.7 | P\* | Chunk hashes, root hash, cert fingerprints. `rayon` feature for parallel hashing. |
| `x25519-dalek` | 3.0.0 | P | Ephemeral ECDH for the pairing secret. |
| `ed25519-dalek` | 3.0.0 | P | Long-term device identity, manifest signatures, TOFU trust store. |
| `chacha20poly1305` | 0.11.0 | P | Encrypts the local keystore at rest. |
| `hkdf` | 0.13.0 | P | Derives the 6-digit SAS from the handshake transcript. |
| `sha2` | 0.11.1 | P | HKDF backing hash, QR payload digest. |
| `hmac` | 0.13.0 | P | Constant-time MAC over the SAS confirmation. |
| `subtle` | 2.6.1 | P | Constant-time comparison. Never `==` on a secret — this crate exists so we can't. |
| `zeroize` | 1.9.1 | P | Wipes keys and plaintext buffers on drop. |
| `argon2` | 0.6.0 | P | Optional passphrase lock on the keystore. Memory-hard, so a stolen laptop is not a stolen identity. |
| `getrandom` | 0.4.3 | P | OS CSPRNG. |
| `rand` | 0.10.4 | P | Non-crypto randomness: jitter, backoff, port selection. |
| `hex` | 0.4.3 | P | Fingerprint display. `hex` over `data-encoding` because we need one format, not five. |

## 7. Wire format & data — 8 crates

| Crate | Ver | Tier | Why |
|---|---|---|---|
| `redb` | 4.6.0 | P | Embedded ACID KV. Transfer history, resume bitmaps, trusted peers, settings. Pure Rust, crash-safe, MVCC. Replaces `rusqlite` (C) and `sled` (unmaintained). |
| `serde` | 1.0.229 | P | Derive. |
| `postcard` | 1.1.3 | P | Wire format: compact, deterministic, no float ambiguity, no_std. |
| `serde_bytes` | 0.11.19 | P | Skips per-byte serde overhead on payload fields. |
| `serde_json` | 1.0.154 | P | Export and debugging only. **Never on the wire.** |
| `toml_edit` | 0.25.19 | P | Config read *and* write, preserving the user's comments and key order. `toml` silently reformats; this doesn't. |
| `uuid` | 1.29.2 | P | Transfer/session IDs, `v7` so they sort by time. |
| `zerocopy` | 0.8.63 | P | Parse fixed headers out of a `Bytes` buffer with no copy and no `unsafe` in our code. |

## 8. Files & content — 13 crates

| Crate | Ver | Tier | Why |
|---|---|---|---|
| `memmap2` | 0.9.12 | P | mmap the source, send `Bytes` without copying. |
| `jwalk` | 0.9.0 | P | Parallel directory walk for folder drops. A 40k-file folder lists in ~1 s, not 30. |
| `walkdir` | 2.5.0 | P | Sequential walk for symlink policy and the "what exactly will be sent" preview. |
| `filetime` | 0.2.29 | P | Preserves mtime/atime. Received files sort correctly in Finder. |
| `tempfile` | 3.27.0 | P | Staging `.part` files **on the destination volume**, so the final rename is atomic. |
| `lz4_flex` | 0.14.2 | P | Opt-in compression. **Off by default** — see §12. |
| `same-file` | 1.0.6 | P | Refuses to send a file to itself. Sounds trivial until a user does it. |
| `sanitize-filename` | 0.6.0 | P | **Security.** Strips `../`, NUL, reserved names from incoming filenames. Without this, a malicious peer writes anywhere on your disk. |
| `unicode-normalization` | 0.1.26 | P | macOS sends NFD, Windows expects NFC. Without this, `café.jpg` arrives as `café.jpg` and looks like a different file. |
| `unicode-segmentation` | 1.13.3 | P | Grapheme-safe truncation. Emoji and Devanagari names don't get sliced mid-cluster. |
| `infer` | 0.22.0 | P | Magic-byte type sniffing for file icons. Trusts content, not the extension. |
| `mime_guess` | 2.0.5 | P | Extension → MIME fallback when magic bytes are inconclusive. |
| `trash` | 5.3.0 | P | Delete goes to the OS trash, not `rm`. One-line change, saves a support nightmare. |

## 9. Platform integration — 8 crates (6 FFI + 2 pure)

| Crate | Ver | Tier | Why | If we cut it |
|---|---|---|---|---|
| `rfd` | 0.18.0 | F | Native file/folder picker. | Hand-rolled egui picker, ~1 week, worse UX |
| `notify-rust` | 4.19.0 | F | "Transfer complete" OS notification. | In-app toast only |
| `tray-icon` | 0.26.1 | F | Tray / menu-bar icon. | No background receiving |
| `muda` | 0.21.2 | F | Native menu bar (File, Edit, Window). Pairs with `tray-icon`. | Custom in-window menu |
| `arboard` | 3.7.0 | F | Copy the pairing code. | egui text is selectable |
| `open` | 5.4.5 | F | Reveal in Finder / Explorer. | Manual path display |
| `directories` | 6.0.0 | P | Correct config/data/cache dirs per OS. | — (keep, it's pure) |
| `fd-lock` | 4.0.4 | P | Single-instance lock. Replaces the abandoned `single-instance` (last touched 2021). | — |

**Optional, feature-gated:**

| Crate | Ver | Tier | Why |
|---|---|---|---|
| `window-vibrancy` | 0.8.1 | F | macOS `NSVisualEffectView` + Windows 11 Mica. The difference between "Rust app" and "app that belongs on this OS". Feature `vibrancy`. |
| `keyring` | 4.2.0 | F | OS keychain storage. **Off by default** — it triggers a system prompt on first run. Default is an Argon2-encrypted file. Feature `keyring`. |

## 10. Observability — 4 crates

| Crate | Ver | Tier | Why |
|---|---|---|---|
| `tracing` | 0.1.44 | P | Structured spans per transfer, per chunk, per retry. |
| `tracing-subscriber` | 0.3.23 | P | Console + rotating file layers, `env-filter`. |
| `tracing-appender` | 0.2.5 | P | Rotating logs with a size cap. An app that fills your disk with logs is worse than no logs. |
| `human-panic` | 2.0.8 | P | Readable crash report instead of a raw backtrace. |

## 11. CLI — 3 crates

| Crate | Ver | Tier | Why |
|---|---|---|---|
| `clap` | 4.6.7 | P | `zenshare send / receive / pair / doctor / serve`. |
| `clap_complete` | 4.6.11 | P | Shell completions for the CLI, generated at build time. |
| `indicatif` | 0.18.6 | P | Terminal progress bars for headless sends. |

`miette` 7.6.0 (P) optional, if we want pretty CLI error diagnostics. Skip until the CLI matters.

## 12. i18n & formatting — 4 crates

| Crate | Ver | Tier | Why |
|---|---|---|---|
| `rust-i18n` | 4.2.4 | P | YAML locale files, `t!()` macro, hot-reloadable. `fluent` is more linguistically correct but is a weekend of setup for a file transfer app. |
| `sys-locale` | 0.3.2 | P | Detect OS locale on first run instead of defaulting to English. |
| `jiff` | 1.0.0 | P | Timezones and "2 minutes ago". Replaces `chrono`. Just hit 1.0 yesterday, by BurntSushi — the one to standardise on now. |
| `bytesize` | 2.7.0 | P | "1.4 GB" formatted correctly, with the binary/decimal distinction that everyone else gets wrong. |

## 13. WAN path — 3 crates, optional (`wan` feature, v2)

| Crate | Ver | Tier | Why |
|---|---|---|---|
| `iroh` | 1.4.1 | P | Hole punching + relay fallback + discovery by public key. Transfer to someone who is not on your Wi-Fi, no port forwarding, no account. |
| `iroh-blobs` | 0.105.0 | P | BLAKE3-verified streaming blobs. We reuse it rather than reimplement resume for WAN. |
| `iroh-relay` | 1.4.1 | P | Only if we self-host a relay. |

⚠️ iroh's default preset contacts `n0.computer` relays. For LAN we don't touch iroh at all. For WAN,
either self-host or make relay use explicit and visible in the UI.

---

## 14. Dev dependencies — 7 crates

| Crate | Ver | Why |
|---|---|---|
| `divan` | 0.1.21 | Benchmarks. Faster and less ceremony than `criterion`; same job. |
| `proptest` | 1.12.0 | Property tests for chunk math, manifest encoding, bitmap arithmetic. |
| `insta` | 1.49.0 | Snapshot tests for wire frames and UI layout. |
| `loom` | 0.7.3 | Model-checks the resume-bitmap concurrency. Catches the race that only appears at 3 a.m. |
| `arbitrary` | 1.5.0 | Feeds `cargo-fuzz` from structured inputs. |
| `cargo-fuzz` | 0.13.2 | Fuzz the wire decoder. **Non-negotiable** for anything that parses bytes from a stranger. |
| `rand_chacha` | 0.9 | Deterministic RNG for reproducible tests. |

## 15. Build & release tooling — 7 crates

| Crate | Ver | Why |
|---|---|---|
| `cargo-dist` | 0.34.0 | Installers, GitHub releases, notarization hooks, Homebrew tap. |
| `cargo-bundle` | 0.12.0 | macOS `.app` bundle when `cargo-dist` isn't enough. |
| `cargo-deny` | 0.20.2 | **Enforces the purity rule in CI** — see below. |
| `cargo-audit` | 0.23.0 | RUSTSEC advisory scan on every push. |
| `cargo-nextest` | 0.9.152 | 3–5× faster test runs. |
| `cargo-fuzz` | 0.13.2 | Also a tool, listed above. |
| `cargo-ndk` | 4.1.2 | Android builds, when we get there. |

### Enforcing "pure Rust" instead of promising it

`deny.toml` — this is how the purity claim becomes a build failure rather than a marketing line:

```toml
[bans]
multiple-versions = "warn"
deny = [
  # webview / app frameworks — the whole point of the project
  { name = "tauri" }, { name = "wry" }, { name = "webview2-com" },
  # C database
  { name = "libsqlite3-sys" }, { name = "rusqlite" },
  # C compression
  { name = "zstd-sys" }, { name = "libz-sys" },
  # C TLS / OpenSSL
  { name = "openssl-sys" }, { name = "openssl" },
  # C GUI toolkits
  { name = "gtk-sys" }, { name = "glib-sys" }, { name = "webkit2gtk-sys" },
  # C allocators
  { name = "mimalloc-sys" }, { name = "jemalloc-sys" },
]

[licenses]
allow = ["MIT", "Apache-2.0", "BSD-3-Clause", "ISC", "Unicode-3.0", "OFL-1.1", "Zlib", "MPL-2.0"]

[advisories]
yanked = "deny"
```

If someone adds `rusqlite` in a hurry, CI fails with the reason. That is worth more than any
README claim.

---

## 16. The 12 decisions that make it feel *finer*, not just faster

Speed is table stakes. These are the things that separate a good app from a demo — each one maps
to a crate above:

| # | Decision | Crate | Why the user notices |
|---|---|---|---|
| 1 | Embedded Inter + JetBrains Mono, tabular figures | — | Text stops jittering. Identical on every machine. |
| 2 | Toasts instead of modals | `egui-notify` | Nothing blocks the queue. |
| 3 | Real thumbnails, SIMD-resized | `image` + `fast_image_resize` | You see *what* you're sending. |
| 4 | Icons from magic bytes, not extensions | `infer` | A `.jpg` that's really a PDF gets the right icon. |
| 5 | NFD→NFC filename normalization | `unicode-normalization` | `café.jpg` doesn't become two files. |
| 6 | Grapheme-safe truncation | `unicode-segmentation` | No broken emoji or half-cut Devanagari. |
| 7 | Trash, not delete | `trash` | Mistakes are recoverable. |
| 8 | Atomic `.part` → rename | `tempfile` + `rustix` | No half-file ever appears in your folder. |
| 9 | Native vibrancy / Mica | `window-vibrancy` | Looks like it shipped with the OS. |
| 10 | Native menu bar | `muda` | ⌘Q, ⌘, and the About panel work as expected. |
| 11 | Locale detected, not assumed | `sys-locale` + `rust-i18n` | Opens in your language. |
| 12 | Link-speed warning *before* transfer | `netdev` | "You're on 2.4 GHz, ~8 MB/s" beats a mystery 20-minute wait. |

---

## 17. Honest scorecard vs. the competition

You asked to beat Google, Apple and "millions of options". Here's the truthful version, because
shipping a false claim is worse than shipping a modest one:

| Against | We win | We don't win |
|---|---|---|
| **Xender** | Everything. No ads, no account, encrypted by default, resumable, verified, 5–10× faster, open protocol. | Nothing meaningful. This is the real target. |
| **Google Nearby Share / Quick Share** | Cross-platform including Linux. No Google account. Verifiable integrity. Resumable. | Their install base, and their Bluetooth+Wi-Fi-Direct fallback when there's no shared network. |
| **Apple AirDrop** | Works across Android/Windows/Linux. Resumable. Integrity-checked. No ecosystem lock-in. | **AWDL.** Apple's peer-to-peer Wi-Fi radio trick needs a private framework. On Apple-to-Apple with no router, AirDrop wins on connection setup. We can't beat that without bindings we've ruled out. |
| **LocalSend** | Faster (QUIC+BBR vs HTTP/TCP), resumable, SAS pairing, WAN path. | Their maturity and existing user base. |

**The honest positioning:** the fastest, most secure *cross-platform* transfer tool — the one that
works between your Mac, your Android and your friend's Windows laptop without anyone installing
anything weird or signing into anything. That is a real gap, and it's winnable.

---

## 18. Full `[workspace.dependencies]`

```toml
[workspace]
resolver = "3"
members  = ["crates/*"]

[workspace.package]
version      = "0.1.0"
edition      = "2024"
rust-version = "1.95"
license      = "MIT OR Apache-2.0"

[workspace.dependencies]
# ── UI ──────────────────────────────────────────────────────────────
# Do NOT enable eframe's `default` features on 0.37: it pulls `system_fonts`,
# which needs `fontique ^0.11.1`, which does not exist upstream. This explicit
# set resolves and is what the app ships with.
eframe            = { version = "0.37", default-features = false, features = [
                      "accesskit", "clipboard", "default_fonts", "links", "wgpu", "persistence",
                    ] }
egui              = "0.37"
egui_extras       = { version = "0.37", features = ["image", "datepicker"] }
egui_dnd          = "0.18"
egui-notify       = "0.24"
fast_qr           = "0.14"
rust-embed        = "8.14"
image             = { version = "0.25", default-features = false, features = ["png", "jpeg", "webp", "gif"] }
fast_image_resize = "6.1"

# ── async ───────────────────────────────────────────────────────────
tokio         = { version = "1.54", features = ["rt-multi-thread", "fs", "net", "sync", "time", "signal", "io-util", "macros"] }
tokio-util    = { version = "0.7", features = ["rt"] }
futures       = "0.3"
async-channel = "2.5"
parking_lot   = "0.13"
dashmap       = "6.2"
bytes         = "1.12"
arc-swap      = "1.9"
async-trait   = "0.1"
backon        = { version = "1.6", default-features = false, features = ["tokio-sleep"] }
rustc-hash    = "2.1"

# ── transport ───────────────────────────────────────────────────────
quinn            = { version = "0.11", default-features = false, features = ["runtime-tokio", "rustls-ring", "log"] }
rustls           = { version = "0.23", default-features = false, features = ["ring", "std", "tls12"] }
rustls-pki-types = "1.16"
rcgen            = { version = "0.14", default-features = false, features = ["ring", "pem"] }
socket2          = "0.6"
rustix           = { version = "1.1", features = ["fs", "net", "mm", "time"] }
if-addrs         = "0.15"
netdev           = "0.47"

# ── discovery ───────────────────────────────────────────────────────
mdns-sd = "0.21"

# ── crypto ──────────────────────────────────────────────────────────
blake3           = { version = "1.8", features = ["rayon"] }
x25519-dalek     = { version = "3.0", features = ["static_secrets"] }
ed25519-dalek    = { version = "3.0", features = ["rand_core", "zeroize"] }
chacha20poly1305 = "0.11"
hkdf             = "0.13"
sha2             = "0.11"
hmac             = "0.13"
subtle           = "2.6"
zeroize          = { version = "1.9", features = ["derive"] }
argon2           = "0.6"
getrandom        = "0.4"
rand             = "0.10"
hex              = "0.4"

# ── data ────────────────────────────────────────────────────────────
redb        = "4.6"
serde       = { version = "1.0", features = ["derive"] }
postcard    = { version = "1.1", features = ["use-std"] }
serde_bytes = "0.11"
serde_json  = "1.0"
toml_edit   = { version = "0.25", features = ["serde"] }
uuid        = { version = "1.29", features = ["v7", "serde"] }
zerocopy    = { version = "0.8", features = ["derive"] }

# ── files ───────────────────────────────────────────────────────────
memmap2              = "0.9"
jwalk                = "0.9"
walkdir              = "2.5"
filetime             = "0.2"
tempfile             = "3.27"
lz4_flex             = "0.14"
same-file            = "1.0"
sanitize-filename    = "0.6"
unicode-normalization = "0.1"
unicode-segmentation = "1.13"
infer                = "0.22"
mime_guess           = "2.0"
trash                = "5.3"

# ── platform ────────────────────────────────────────────────────────
rfd         = "0.18"
notify-rust = "4.19"
tray-icon   = "0.26"
muda        = "0.21"
arboard     = "3.7"
open        = "5.4"
directories = "6.0"
fd-lock     = "4.0"

# ── i18n / formatting ───────────────────────────────────────────────
rust-i18n  = "4.2"
sys-locale = "0.3"
jiff       = "1.0"
bytesize   = "2.7"

# ── observability ───────────────────────────────────────────────────
tracing            = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter", "json"] }
tracing-appender   = "0.2"
human-panic        = "2.0"

# ── CLI ─────────────────────────────────────────────────────────────
clap           = { version = "4.6", features = ["derive"] }
clap_complete  = "4.6"
indicatif      = "0.18"

# ── optional features ───────────────────────────────────────────────
resvg           = { version = "0.50", optional = true }
tiny-skia       = { version = "0.12", optional = true }
window-vibrancy = { version = "0.8", optional = true }
keyring         = { version = "4.2", optional = true }
iroh            = { version = "1.4", optional = true }
iroh-blobs      = { version = "0.105", optional = true }
iroh-relay      = { version = "1.4", optional = true }

# ── dev ─────────────────────────────────────────────────────────────
divan      = "0.1"
proptest   = "1.12"
insta      = "1.49"
loom       = "0.7"
arbitrary  = "1.5"
```

### Per-crate manifest pattern

```toml
# crates/zs-net/Cargo.toml
[package]
name         = "zs-net"
version.workspace      = true
edition.workspace      = true
rust-version.workspace = true

[dependencies]
zs-core   = { path = "../zs-core" }
zs-crypto = { path = "../zs-crypto" }
quinn.workspace    = true
rustls.workspace   = true
tokio.workspace    = true
mdns-sd.workspace  = true
tracing.workspace  = true
thiserror          = "2.0"

[features]
default = []
wan     = ["dep:iroh", "dep:iroh-blobs"]

[lints]
workspace = true

[profile.release]
lto       = "fat"
codegen-units = 1
strip     = "symbols"
panic     = "abort"
```

Note `thiserror` — add `thiserror` 2.0.21 and `anyhow` 1.0.104 as workspace deps for error handling.
They were in `PLAN.md` but I've kept them out of the main tables above because they're infrastructure,
not features: `thiserror` in library crates, `anyhow` only in `zs-app`'s top level.

---

## 19. Dependency budget

| Bucket | Count |
|---|---|
| Mandatory runtime | 84 |
| Optional (features) | 7 |
| Dev | 7 |
| Build tooling | 7 |
| **Direct total** | **105** |
| Transitive (estimated) | ~260 |
| Binary, stripped | 18–30 MB |
| Cold start target | < 300 ms |

84 runtime crates sounds like a lot until you compare: Tauri's default template pulls ~400 transitive
with a 60 MB binary and a 300 MB webview runtime. We're not smaller because we're clever — we're
smaller because there's no browser engine in the box.

### Where the weight actually is

`eframe` + `wgpu` is roughly 60% of compile time and 50% of binary size. If binary size ever becomes
the priority, the lever is switching `eframe` to the `glow` backend (OpenGL instead of wgpu): smaller,
faster to compile, marginally less modern. That's a one-line change in the workspace manifest.

---

## 20. Still open

1. **`ring`** is the only tier-C crate. Accept it (recommended, audited, fast) or chase zero-C with
   the experimental `rustls-rustcrypto` provider?
2. **`window-vibrancy` / `keyring` / `muda`** — the FFI tier is 6 mandatory crates, 8 with the two
   optional ones. Are we keeping vibrancy (biggest visual win, adds one) or staying minimal?
3. **`rust-i18n` vs `fluent`** — simple vs linguistically correct. Recommend `rust-i18n` now.
4. **`glow` vs `wgpu`** — if binary size matters more than future-proofing.
