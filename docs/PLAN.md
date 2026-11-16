# ZenShare — Planning Document

**Goal:** a Xender-class local file transfer app, rebuilt in Rust. Faster, safer, encrypted by default, zero ads, zero telemetry, zero accounts.

**Status:** phases 0–2 built. Engine verified end to end; UI live. See §10 for what
changed once the code met reality.
**Toolchain:** rustc 1.99.0 / cargo 1.99.0, edition 2024.
**Versions verified against crates.io on 2026-11-16.**

---

## 0. What we are actually beating

Xender's real weaknesses, in order of how much we can win:

| Xender problem | Our answer |
|---|---|
| Ads, upsells, telemetry, account nags | None. Local-only. No network calls except to peers. |
| Weak/unclear crypto on transfer | QUIC + TLS 1.3 always, plus an out-of-band 6-digit SAS check that defeats MITM |
| One TCP stream, no resume — drop Wi-Fi at 90% and start over | Multi-stream QUIC, per-chunk bitmap resume, reconnect continues mid-file |
| No integrity proof — silent corruption | BLAKE3 per 4 MiB chunk, verified inline, `.part` → atomic rename |
| Same-network only | LAN fast path (mDNS) **plus** WAN path (iroh hole punching) — send to a friend who is not on your Wi-Fi |
| Slow: 5–10 MB/s typical | Target: saturate the link — 60–110 MB/s on Wi-Fi 6, 110+ MB/s on 1 GbE |
| Bloated Electron/native hybrid shell | ~15–25 MB single binary, native GPU-rendered UI, cold start < 300 ms |

---

## 1. GUI decision — the one that shapes everything

You asked for the *most native and stable* crate. Here is the honest landscape, because the
premise needs one correction up front:

> **No pure-Rust GUI crate gives you real OS-native widgets.** Native widgets = calling
> AppKit / Win32 / GTK directly = bindings. The moment you demand "pure Rust, no bindings",
> you accept a self-drawn, GPU-rendered UI. That is not a downgrade — Xender, Discord, Slack,
> Figma and Zed all ship self-drawn UIs. "Native" in 2026 means *native binary, native
> performance, no webview*, not *native widgets*.

### Contenders

| | **egui / eframe 0.37** | **iced 0.14** | **gpui 0.2.2** | slint 1.18 | dioxus / freya / xilem |
|---|---|---|---|---|---|
| Pure Rust, no DSL | ✅ | ✅ | ✅ | ❌ own `.slint` DSL | ✅ |
| Latest release | 2026-10-15 | 2025-12-07 | **~1 year stale** | 2026-09-21 | mixed |
| Downloads | 31.7M | 3.4M | 0.5M | 2.6M | <0.4M |
| API stability | Low (breaking each minor) | Medium | Low, 0.2.x | Medium | Low |
| Desktop | mac/win/linux | mac/win/linux | mac/linux/win | all | desktop |
| **Mobile (Android/iOS)** | ✅ via eframe + android-activity | ⚠️ weak | ❌ none | ✅ | ❌ |
| Ecosystem | Huge (extras, dnd, plot, wgpu) | Medium | Small | Small | Tiny |
| Time to first working UI | Hours | Days | Weeks | Days | Weeks |
| Perf | Good | Good | Best | Good | Good |

### Verdict: **egui 0.37 + eframe 0.37**

Reasons that actually matter for *this* app:

1. **The UI is a live dashboard, not a form.** Device list + a queue of files with per-file
   progress bars updating 60×/sec. That is immediate mode's best case. In a retained framework
   you fight diffing and state plumbing for zero benefit.
2. **Only one with a real mobile story.** Xender is a phone app first. eframe runs on Android
   (`android-activity`, `ndk`) and iOS (`winit` + `objc2`) — imperfect, but it exists. iced's
   mobile support is not there, gpui's is nonexistent.
3. **31.7M downloads vs 3.4M.** When something breaks at 2 a.m., the answer exists.
4. **gpui is not ready.** 0.2.2 is the newest release and it is roughly a year old, docs are
   thin, it is Zed's internal framework published for convenience, and Linux support has
   historically lagged. Revisit at 1.0 — the architecture below lets us swap it.

**Cost we accept:** egui's default theme looks like egui. We will ship a custom theme
(dark, rounded, brand accent) in `zs-ui/theme.rs`. ~2 days of work, and it is the difference
between "Rust demo" and "product".

**Insurance:** all UI lives in `zs-ui` behind a `ViewModel` boundary. Swapping to iced later
touches one crate, not the app.

---

## 2. Transport decision

Two paths, one protocol above them.

**LAN path (default): `quinn` 0.11.12**
- QUIC over UDP. TLS 1.3 built in, so encryption is not an add-on we can forget.
- Per-stream independence → 8 files transfer in parallel with no head-of-line blocking.
- `quinn-proto` ships a **BBR** congestion controller plus pacing — this is the single biggest
  throughput win over TCP-over-Wi-Fi, which collapses on loss.
- Connection migration: phone flips Wi-Fi → hotspot, the connection survives. Xender dies here.
- 0-RTT reconnect → resume is instant.

**WAN path (feature `wan`, v2): `iroh` 1.4.1**
- Same QUIC core, adds hole punching, relay fallback, and discovery by public key.
- Lets two people transfer across the internet with no port forwarding and no account.
- `iroh-blobs` 0.105 gives BLAKE3-verified streaming blobs — we reuse it for the WAN path only.
- Caveat: default presets phone home to `n0.computer` relays. For LAN we do **not** use iroh at
  all; for WAN we either self-host `iroh-relay` or make relay use explicit and disclosed.

**Why not TCP:** no built-in TLS, one stream = head-of-line blocking, no migration, and we'd
hand-roll framing anyway. QUIC is strictly better here and quinn is the mature pure-Rust impl.

---

## 3. The "pure Rust" accounting — read this before promising anything

There are three tiers. We should agree on which one we are selling.

**Tier A — no foreign runtime (what we actually deliver).**
No webview, no Electron, no C++ framework, no Qt/GTK, no Python, no Node, no JNI unless we
target Android. Everything in the process is Rust, except audited low-level crypto asm.

**Tier B — zero C/asm at all.** Possible, but costs us: `rustls` + `ring`/`aws-lc-rs` is the
only production-grade TLS for QUIC. The pure-Rust alternative is the `rustls-rustcrypto`
provider — 0.0.x, experimental, slower, far less audited. For a *security* product, choosing an
unaudited crypto provider to win a purity argument is the wrong trade. **Recommendation: stay on
`ring`, and say so publicly.**

**Tier C — zero platform API calls.** Impossible for a desktop app. Any app that opens a file
dialog, shows a notification, or sits in the tray is calling the OS. We contain this in one
crate and keep it to five small crates (below).

**The six honest exceptions** (all pure-Rust *APIs*, platform FFI underneath). The authoritative,
version-pinned list lives in [`CRATES.md` §9](./CRATES.md):

| Crate | Why it can't be avoided | Alternative if we must |
|---|---|---|
| `rfd` 0.18 | native file/folder picker | write our own picker in egui (worse UX, ~1 week) |
| `notify-rust` 4.19 | "Transfer complete" OS notification | skip; show in-app only |
| `tray-icon` 0.26 | menu bar / system tray | `ksni` (zbus, pure Rust) on Linux + `objc2`/`windows` by hand |
| `muda` 0.21 | native menu bar (⌘Q, ⌘, About) | custom in-window menu |
| `arboard` 3.7 | copy the pairing code | egui text is selectable; skip |
| `nokhwa` 0.10 | camera QR scan (v2, feature-gated) | 6-digit code entry — **v1 ships without camera** |

Optional additions, both feature-gated: `window-vibrancy` 0.8 (macOS vibrancy / Windows 11 Mica) and
`keyring` 4.2 (OS keychain — off by default, because it triggers a system prompt).

`ring` is the seventh and the only one that touches the security path. That is the whole list.
Everything else in this document is 100% Rust, top to bottom.

---

## 4. Full crate manifest

Every version below was pulled from crates.io today. This is the complete dependency set —
if a crate is not on this list, we are not using it.

### 4.1 UI

| Crate | Ver | Role |
|---|---|---|
| `eframe` | 0.37.0 | app shell: window, event loop, wgpu/glow surface, native menu, mobile entry |
| `egui` | 0.37.0 | immediate-mode UI, layout, input, drag-drop |
| `egui_extras` | 0.37.0 | tables, date pickers, image loaders |
| `egui_dnd` | 0.18.0 | drag-to-reorder the transfer queue |
| `fast_qr` | 0.14.0 | QR matrix generation — pure Rust, no image dep; we draw the modules as egui rects |

Do **not** add `wgpu`, `winit`, or `egui-wgpu` directly. eframe pins compatible versions; adding
them manually creates version conflicts on every eframe upgrade.

### 4.2 Async runtime & plumbing

| Crate | Ver | Role |
|---|---|---|
| `tokio` | 1.54.1 | runtime (`rt-multi-thread`, `fs`, `net`, `sync`, `time`, `signal`, `io-util`, `macros`) |
| `tokio-util` | 0.7.20 | `CancellationToken` for per-transfer cancel, `codec` helpers |
| `futures` | 0.3.34 | stream combinators |
| `async-channel` | 2.5.0 | bounded UI ↔ engine command channel |
| `parking_lot` | 0.13.0 | fast locks in hot paths |
| `dashmap` | 6.2.1 | concurrent peer/transfer registry |
| `bytes` | 1.12.1 | zero-copy `Bytes` slices shared between mmap and QUIC |

### 4.3 Transport

| Crate | Ver | Role |
|---|---|---|
| `quinn` | 0.11.12 | QUIC endpoint, streams, BBR, pacing, 0-RTT |
| `rustls` | 0.23.46 | TLS 1.3 (`ring` provider) |
| `rustls-pki-types` | 1.16.0 | cert/key types shared with rcgen |
| `rcgen` | 0.14.10 | mint a self-signed cert per session at runtime |
| `socket2` | 0.6.5 | 4–16 MiB SO_RCVBUF/SNDBUF, address reuse, bind control |
| `rustix` | 1.1.6 | pure-Rust syscalls: `pwritev`, `fadvise`, `fallocate` (Linux/mac), no libc crate |
| `if-addrs` | 0.15.0 | enumerate interfaces, pick the LAN IPv4/IPv6 |
| `netdev` | 0.47.0 | detect link type/speed so we can warn "you're on a hotspot at 2.4 GHz" |

### 4.4 Discovery

| Crate | Ver | Role |
|---|---|---|
| `mdns-sd` | 0.21.7 | pure-Rust mDNS/DNS-SD browse + advertise `_zenshare._udp.local` |
| `fast_qr` | 0.14.0 | QR payload for cross-network / locked-down-Wi-Fi pairing |

mDNS is **discovery only, never trust**. Anyone on the network can spoof an advertisement —
that is exactly why the SAS code exists.

### 4.5 Crypto

| Crate | Ver | Role |
|---|---|---|
| `blake3` | 1.8.7 | chunk hashes, root hash, cert fingerprints (`rayon` feature for parallel hashing) |
| `x25519-dalek` | 3.0.0 | ephemeral ECDH for the pairing secret |
| `ed25519-dalek` | 3.0.0 | long-term device identity (sign manifests, TOFU trust store) |
| `chacha20poly1305` | 0.11.0 | encrypting the local keystore at rest |
| `hkdf` | 0.13.0 | derive the 6-digit SAS from the handshake transcript |
| `sha2` | 0.11.1 | HKDF backing hash, QR payload digest |
| `hmac` | 0.13.0 | constant-time MAC for the SAS confirmation |
| `subtle` | 2.6.1 | constant-time comparison — never `==` on secrets |
| `zeroize` | 1.9.1 | wipe keys and plaintext buffers on drop |
| `argon2` | 0.6.0 | optional passphrase lock on the keystore |
| `getrandom` | 0.4.3 | OS CSPRNG |
| `rand` | 0.10.4 | non-crypto randomness (jitter, backoff) |

### 4.6 Storage & data

| Crate | Ver | Role |
|---|---|---|
| `redb` | 4.6.0 | embedded ACID KV — transfer history, resume bitmaps, trusted peers, settings |
| `serde` | 1.0.229 | derive |
| `postcard` | 1.1.3 | wire format: compact, no_std, no float ambiguity |
| `serde_bytes` | 0.11.19 | avoid per-byte serde overhead on payloads |
| `serde_json` | 1.0.154 | export/debug only, never on the wire |
| `toml` | 1.2.1 | human-editable config file |
| `uuid` | 1.29.2 | transfer/session IDs (`v7` for time-sortable) |

`redb` over `sled` (unmaintained) and over `rusqlite` (**C**). It is pure Rust, ACID, MVCC,
and crash-safe — exactly what resume state needs.

### 4.7 File I/O

| Crate | Ver | Role |
|---|---|---|
| `memmap2` | 0.9.12 | mmap the source file → send `Bytes` without copying |
| `jwalk` | 0.9.0 | parallel directory walk for folder drops |
| `walkdir` | 2.5.0 | simple sequential walk (fallback / symlink policy) |
| `filetime` | 0.2.29 | preserve mtime/atime on receive |
| `tempfile` | 3.27.0 | staging `.part` files in the destination volume |
| `lz4_flex` | 0.14.2 | opt-in compression, off by default (see §7) |

### 4.8 Platform glue (the five exceptions)

| Crate | Ver | Role |
|---|---|---|
| `rfd` | 0.18.0 | native file/folder dialogs |
| `notify-rust` | 4.19.0 | OS notifications |
| `tray-icon` | 0.26.1 | tray / menu-bar icon |
| `arboard` | 3.7.0 | clipboard (copy pairing code) |
| `open` | 5.4.5 | reveal received file in Finder/Explorer |
| `directories` | 6.0.0 | correct config/data/cache dirs per OS |
| `fd-lock` | 4.0.4 | single-instance lock (replaces stale `single-instance` crate, last updated 2021) |

### 4.9 Observability & CLI

| Crate | Ver | Role |
|---|---|---|
| `tracing` | 0.1.44 | structured spans per transfer/chunk |
| `tracing-subscriber` | 0.3.23 | console + file layers, env filter |
| `tracing-appender` | 0.2.5 | rotating log files, capped |
| `human-panic` | 2.0.8 | readable crash report instead of a backtrace |
| `clap` | 4.6.7 | `zenshare send/receive/serve/pair/doctor` subcommands |

### 4.10 WAN (feature `wan`, off by default)

| Crate | Ver | Role |
|---|---|---|
| `iroh` | 1.4.1 | hole punching + relay fallback + key-based discovery |
| `iroh-blobs` | 0.105.0 | BLAKE3-verified streaming blob transfer for the WAN path |
| `iroh-relay` | 1.4.1 | only if we self-host a relay |

### 4.11 Dev only

`criterion` (bench), `proptest` (protocol fuzz), `insta` (snapshot UI/format tests),
`cargo-nextest`. No runtime cost.

### Deliberately rejected

| Crate | Why not |
|---|---|
| `tauri` | webview. Disqualified by the brief. |
| `rusqlite` / `libsqlite3-sys` | C. `redb` covers our needs. |
| `zstd` | C bindings. `ruzstd` is decode-only. `lz4_flex` is pure Rust and faster. |
| `sled` | effectively unmaintained, beta for years. |
| `single-instance` | abandoned since 2021 → `fd-lock`. |
| `mimalloc` | C. eframe docs claim ~20% gain, but it breaks Tier A. Measure first; `talc` is the pure-Rust fallback if we truly need it. |
| `slint` | own DSL — UI is not Rust. |
| `gpui` | 0.2.2, ~1 year stale, no mobile. Revisit at 1.0. |

---

## 5. Workspace layout

```
zenshare/
├─ Cargo.toml              # [workspace] + [workspace.dependencies]
├─ docs/PLAN.md
└─ crates/
   ├─ zs-core/             # types, protocol messages, manifest, chunk math, errors
   ├─ zs-crypto/           # device identity, SAS pairing, keystore, trust store
   ├─ zs-net/              # quinn endpoint, mDNS discovery, session FSM, [wan] iroh
   ├─ zs-transfer/         # sender/receiver pipelines, resume, verify, disk I/O
   ├─ zs-store/            # redb tables: history, bitmaps, peers, settings
   ├─ zs-ui/               # egui views + ViewModel (never calls net directly)
   └─ zs-app/              # binary: wiring, tray, clap subcommands
```

Dependency direction is strictly one-way — no cycles:

```
zs-app ──> zs-ui ──> zs-core
   │  └──> zs-net ──> zs-crypto ──> zs-core
   │  └──> zs-transfer ──> zs-net, zs-store
   └─────> zs-store ──> zs-core
```

`zs-ui` communicates with `zs-app` through:
- **down:** `Arc<RwLock<ViewModel>>` — UI reads a snapshot each frame, never blocks
- **up:** `async_channel::Sender<Command>` — bounded, so a dead engine backpressures the UI

This is the boundary that makes an iced/gpui swap a one-crate change.

### `[workspace.dependencies]` — copy-paste ready

```toml
[workspace]
resolver = "3"
members = ["crates/*"]

[workspace.package]
version      = "0.1.0"
edition      = "2024"
rust-version = "1.95"
license      = "MIT OR Apache-2.0"

[workspace.dependencies]
# ui
eframe       = { version = "0.37", default-features = false, features = ["wgpu", "persistence", "accesskit"] }
egui         = "0.37"
egui_extras  = "0.37"
egui_dnd     = "0.18"
fast_qr      = "0.14"

# runtime
tokio        = { version = "1.54", features = ["rt-multi-thread", "fs", "net", "sync", "time", "signal", "io-util", "macros"] }
tokio-util   = { version = "0.7", features = ["rt"] }
futures      = "0.3"
async-channel = "2.5"
parking_lot  = "0.13"
dashmap      = "6.2"
bytes        = "1.12"

# transport
quinn              = { version = "0.11", default-features = false, features = ["runtime-tokio", "rustls-ring", "log"] }
rustls             = { version = "0.23", default-features = false, features = ["ring", "std", "tls12"] }
rustls-pki-types   = "1.16"
rcgen              = { version = "0.14", default-features = false, features = ["ring", "pem"] }
socket2            = "0.6"
rustix             = { version = "1.1", features = ["fs", "net", "mm", "time"] }
if-addrs           = "0.15"
netdev             = "0.47"

# discovery
mdns-sd      = "0.21"

# crypto
blake3             = { version = "1.8", features = ["rayon"] }
x25519-dalek       = { version = "3.0", features = ["static_secrets"] }
ed25519-dalek      = { version = "3.0", features = ["rand_core", "zeroize"] }
chacha20poly1305   = "0.11"
hkdf               = "0.13"
sha2               = "0.11"
hmac               = "0.13"
subtle             = "2.6"
zeroize            = { version = "1.9", features = ["derive"] }
argon2             = "0.6"
getrandom          = "0.4"
rand               = "0.10"

# data
redb         = "4.6"
serde        = { version = "1.0", features = ["derive"] }
postcard     = { version = "1.1", features = ["use-std"] }
serde_bytes  = "0.11"
serde_json   = "1.0"
toml         = "1.2"
uuid         = { version = "1.29", features = ["v7", "serde"] }

# files
memmap2      = "0.9"
jwalk        = "0.9"
walkdir      = "2.5"
filetime     = "0.2"
tempfile     = "3.27"
lz4_flex     = "0.14"

# platform
rfd          = "0.18"
notify-rust  = "4.19"
tray-icon    = "0.26"
arboard      = "3.7"
open         = "5.4"
directories  = "6.0"
fd-lock      = "4.0"

# observability
tracing             = "0.1"
tracing-subscriber  = { version = "0.3", features = ["env-filter", "json"] }
tracing-appender    = "0.2"
human-panic         = "2.0"
clap                = { version = "4.6", features = ["derive"] }

# optional wan
iroh         = { version = "1.4", optional = true }
iroh-blobs   = { version = "0.105", optional = true }
```

---

## 6. Protocol sketch

### 6.1 Discovery
`mdns-sd` advertises `_zenshare._udp.local` with TXT: `id` (device pubkey fingerprint),
`name`, `ver`, `caps`, `port`. Browse gives us a live device list in <200 ms on a normal LAN.

### 6.2 Handshake
1. QUIC connect. Each side presents a runtime-minted self-signed cert (`rcgen`).
2. Client **pins** the expected cert fingerprint when it came from a QR code; when it came from
   mDNS (untrusted) it proceeds to pairing instead.
3. Ephemeral X25519 exchange on a control stream.
4. Both sides compute `SAS = HKDF(shared_secret, transcript_hash) mod 10^6` → a 6-digit number.
   Both screens show it. User confirms they match on **one** side.
5. On success: store the peer's Ed25519 pubkey (TOFU, like `known_hosts`). Future transfers to
   that device skip the code entirely.

This defeats mDNS spoofing, which is the actual attack on a café Wi-Fi. Xender has no equivalent.

### 6.3 Transfer
- File → **4 MiB chunks**. Chunk size tuned per link (2 MiB on 2.4 GHz, 8 MiB on 1 GbE).
- Sender opens **8 unidirectional QUIC streams** and round-robins chunks across them.
- Each chunk frame: `[u32 idx][u32 len][payload][32-byte blake3]`.
- Receiver verifies the hash **before** writing. Bad chunk → request retry on the control stream,
  three strikes → fail that file only, not the batch.
- Root hash = BLAKE3 over the concatenated chunk hashes, sent in a final `Commit` frame.
  **No pre-hash pass** — sender hashes while reading, so a 10 GB file starts moving instantly.
- Receiver writes with positioned `pwrite` (via `rustix` on unix, `seek_write` on Windows) into
  `<name>.part`, `fsync`s, then atomically renames. No half-written files ever appear.
- Resume: receiver persists a chunk bitmap in `redb` every ~1 s. On reconnect it sends the bitmap;
  sender seeks directly to the missing ranges.

### 6.4 Speed plan — where the 10× comes from

| Lever | Effect |
|---|---|
| BBR instead of CUBIC/TCP | 2–4× on lossy Wi-Fi. The dominant factor. |
| 8 parallel streams | Overlaps RTT stalls; no head-of-line blocking |
| 16 MiB socket buffers (`socket2`) | Keeps the pipe full on high-BDP links |
| UDP GSO/GRO (inside `quinn-udp`) | Fewer syscalls, less CPU per byte |
| `memmap2` + `Bytes` | Zero-copy send path — no userland buffer copy |
| BLAKE3 on `rayon` threads | Hashing overlaps network I/O, not serialized |
| Compression **off** | Wi-Fi is the bottleneck, not the CPU. Compressing first makes it slower. |

Realistic expectation: on Wi-Fi 6 with both devices close, **60–110 MB/s**. On 1 GbE, **110+ MB/s**,
i.e. disk-bound. We should publish a benchmark harness (`criterion` + a loopback mode) and put real
numbers in the README rather than marketing claims.

---

## 7. What we deliberately do *not* build in v1

Scope discipline is the difference between shipping and not.

- **No camera QR scanning.** 6-digit code + QR *display* covers pairing. Camera needs `nokhwa`
  and per-platform permissions; it is a v2 feature flag.
- **No Wi-Fi Direct / hotspot creation.** macOS cannot script Internet Sharing; Android needs JNI
  into `WifiP2pManager`. v1 uses "be on the same Wi-Fi"; v2 adds a guided hotspot flow.
- **No browser receiver.** Tempting ("no install!") but browsers can't do raw QUIC, and an HTTP
  fallback quietly downgrades security. Revisit as an explicitly-labelled, opt-in HTTPS mode.
- **No accounts, no cloud, no sync, no chat.** That is a different product.
- **No Android/iOS in v1.** Ship desktop first; the UI crate is mobile-ready when we get there.

---

## 8. Phased roadmap

**Phase 0 — skeleton (2–3 days)**
Workspace, CI, `tracing`, `clap` with `doctor` subcommand, egui window that renders a device list
from a fake `ViewModel`. Proves the UI story before any protocol work.

**Phase 1 — LAN transfer, no crypto ceremony (1–2 weeks)**
quinn echo, mDNS discovery, single-file send with chunking + BLAKE3 verify, progress bar.
Hardcode trust. Success = 1 GB file, hash matches, >50 MB/s on LAN.

**Phase 2 — real security (1 week)**
`zs-crypto`: identity, rcgen certs, X25519, SAS, TOFU store, `zeroize` everywhere.
Success = MITM attempt on a spoofed mDNS record is caught by the SAS mismatch.

**Phase 3 — product hardening (2 weeks)**
Resume across disconnect, multi-file queue, folder send, cancel, `redb` history, tray,
notifications, atomic rename, disk-full handling, `doctor` diagnostics.

**Phase 4 — polish + ship (1–2 weeks)**
Custom theme, empty states, error copy, `human-panic`, code signing/notarization, benchmark
harness, README with real numbers.

**Phase 5 — WAN + mobile (post-1.0)**
`iroh` feature, then Android via `android-activity` + JNI for Wi-Fi Direct.

---

## 9. Open decisions — need your call

1. **Purity tier:** ship on `ring` (Tier A, audited, fast) or chase zero-C with
   `rustls-rustcrypto` (Tier B, unaudited, slower)? **I recommend Tier A.**
2. **First platform:** macOS-only first (fastest iteration, you're on darwin) or
   all three desktops from day one (3× the CI pain)?
3. **Mobile:** is Android a 1.0 requirement or a 2.0 feature? It changes the GUI risk profile.
4. **Name:** `zenshare` is the crate name. Keep it, or is that a placeholder?

---

## 10. Implementation notes — what changed once the code met reality

Recorded because a plan that never gets corrected is fiction.

**egui 0.37 is not the egui of the docs you remember.** The `App` trait's required
method is now `fn ui(&mut self, ui: &mut Ui, frame: &mut Frame)`, not `update(&mut
self, ctx, frame)`. Panels take `&mut Ui`, not `&Context`. `Ui::id` became
`Ui::scope_id`, `Rounding` became `CornerRadius` (u8, not f32), `set_style` became
`set_global_style`/`all_styles_mut`. This is why the UI paints onto one canvas and
uses `ui.interact(rect, id, sense)` for hit-testing: it touches almost none of the
churny surface, and the parts that did change were one-line fixes.

**`eframe`'s default features are broken on 0.37.** `default` includes
`system_fonts`, which pulls `egui_system_font_provider`, which requires
`fontique ^0.11.1` — a version that does not exist on crates.io. Defaults fail to
resolve. The working set is explicit:

```toml
eframe = { version = "0.37", default-features = false, features = [
  "accesskit", "clipboard", "default_fonts", "links", "wgpu", "persistence",
] }
```

Dropping `default-features = false` entirely is not an option until upstream fixes
the pin.

**`rustix::fs::pwrite` moved to `rustix::io::pwrite`** in the 1.x line.

**`quinn::ServerConfig::with_single_cert` does not set ALPN.** The handshake fails
with `error 120: peer doesn't support any known protocol`. Both sides must build
their rustls config by hand and set `alpn_protocols`.

**Discovery is multicast, and multicast is fragile.** Admin-scoped group
`239.255.42.99`, TTL 1, plus a loopback unicast duplicate so two instances on one
machine still find each other. Joining the group can fail on hosts with no
multicast-capable interface; that is now a warning, not a fatal error. This is
also why `zs-net` keeps `mdns-sd` in the dependency list but does not use it yet:
if multicast proves unreliable in the field, mDNS is the drop-in upgrade and the
rest of the protocol does not change.

**`egui` only repaints on input.** A transfer completing while the user does not
move the mouse would leave a stale window. The app now requests a repaint every
80 ms while something is moving and every 250 ms when idle. Verified by counting
frames, not by assuming.

**One whole-file hash pass is unnecessary.** Per-chunk BLAKE3 plus fixed chunk
geometry means a fully verified chunk set *is* a verified file. The planned
`Commit { root }` frame was dropped; `Commit` now carries only a byte count. A
10 GB transfer starts on the first chunk instead of after a pre-hash.

**Speed numbers must be measured, not claimed.** The debug build moves 64 MiB at
28 MiB/s; that number is meaningless because blake3 and the write path are
unoptimised. Only release numbers belong in the README.
