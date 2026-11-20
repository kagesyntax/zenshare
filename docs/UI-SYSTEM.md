# ZenShare UI system

A design specification and refactor plan for the `egui` front end. Written before
any code changes, so the decisions can be argued with while they are still cheap.

---

## 1. Audit of the current interface

Evidence: `crates/zs-ui/src/{lib,theme,widgets}.rs` (1,433 lines) and
`docs/screenshot.png`.

### 1.1 What is already right

- **Custom painting instead of widgets.** Every control is `ui.interact(rect, id,
  sense)` over a computed rect. This is the single most important decision in the
  codebase and it should survive the refactor untouched: it is what makes a
  pixel-exact HIG layout possible at all, and it is what made the egui 0.37
  migration a one-hour job instead of a rewrite.
- **A token file exists.** `theme.rs` already separates colour from layout, which
  is more than most egui apps manage.
- **Embedded fonts.** `include_bytes!` Inter + JetBrains Mono. No system fallback
  roulette.
- **Monospace for numbers.** Byte counters do not twitch. Keep.

### 1.2 What is wrong

| # | Finding | Evidence |
|---|---|---|
| 1 | **No type scale.** Eleven distinct sizes in use: 10, 11, 12, 12.5, 13, 13.5, 14.5, 15, 16, 18, 26. Each call site picks its own. | `theme.rs` takes bare `f32`; call sites pass literals |
| 2 | **Card soup.** `card_bordered` draws the device rows, the transfer rows, the drop zone, *and* the Send container that holds the drop zone. Card inside card inside card. | `lib.rs:459,562,653` |
| 3 | **Broken baseline.** "Nearby" and "Send" sit 10px apart vertically because each panel computes its own header offset. | `lib.rs:126` vs `136` in the screenshot |
| 4 | **Panels overlap by construction.** `devices_panel` and `transfers_panel` are handed the *same* rect; transfers is bottom-anchored inside it. Three or more transfers collide with the device list. | `lib.rs:367-383` |
| 5 | **Zero interaction states.** No hover, pressed, selected or focus feedback anywhere. `ui.interact` is called, its response is often discarded, and nothing visual changes. HIG is mostly *about* states. | grep `\.hovered\(\)` → 0 hits |
| 6 | **Inverted hierarchy.** The largest, highest-contrast element on screen is the disabled "Select a device and files" button. The loudest thing is the one you cannot use. | screenshot, bottom right |
| 7 | **Duplicated information.** "2 devices nearby" appears in the header and again in the status bar. | screenshot, top right + bottom left |
| 8 | **Double chrome.** Native macOS titlebar, then a separate 76px custom header below it. Two horizontal bands doing one job. | screenshot |
| 9 | **Dead token.** `ACCENT_ALT` (periwinkle) is declared and never used. Two accents promised, one delivered. | `theme.rs:29` |
| 10 | **No motion.** State changes are instantaneous cuts. The only timer is the repaint tick. | `lib.rs:404` |
| 11 | **Hardcoded layout.** `pad 26`, `header 76`, sidebar `38%` clamped to 320–400. No grid, no relationship between these numbers. | `lib.rs:355-365` |
| 12 | **One 953-line file** holding five panels, the app state, and the model. | `wc -l` |

### 1.3 The uncomfortable one

The current palette — mint `#3BE8B0` on near-black `#0A0C0F` — is the default
"AI product" look. Teal on charcoal is the single most generated colour scheme in
circulation. It is not *bad*, but it is not a point of view, and it reads as
defaulted rather than decided.

Recommendation: **promote the periwinkle already in the file (`#7C8CFF`) to the
interactive accent, and demote mint to a state colour** meaning "data moving".
That gives one interactive colour, one state colour, and uses what is already
there instead of inventing a third.

---

## 2. Design system blueprint

### 2.1 Type scale

macOS body text is **13pt**, not 16. Everything below derives from that.

| Token | Size / line | Weight | Used for |
|---|---|---|---|
| `Title1` | 22 / 28 | Semibold | App name in the titlebar |
| `Title2` | 17 / 22 | Semibold | Panel titles ("Send", "Transfers") |
| `Headline` | 13 / 18 | Semibold | Row titles, button labels |
| `Body` | 13 / 18 | Regular | Primary text |
| `Callout` | 12 / 16 | Regular | Secondary text, hints |
| `Caption` | 11 / 14 | Regular | Metadata, addresses |
| `Label` | 10 / 13 | Medium | Section tags, +6% tracking, uppercase |
| `Mono` | 12 / 16 | Regular | Byte counters, speeds, hashes |

Eight steps, no others. Anything that wants a ninth is a component that needs
redesigning, not a new size.

### 2.2 Spacing

4pt base, 8pt rhythm. Named, never literal:

```
S1 4   S2 8   S3 12   S4 16   S5 24   S6 32   S7 48
```

Derived rules:
- Panel padding: `S5` (24)
- Gap between panels: `S4` (16)
- Row inner padding: `S3` horizontal, `S2` vertical
- Section header → first row: `S3`
- Icon → label: `S3`
- Two-line row: `S1` between lines

### 2.3 Radii

| Token | Value | Used for |
|---|---|---|
| `R_CONTROL` | 6 | Buttons, chips, inputs |
| `R_CARD` | 10 | Rows, cards, drop zone |
| `R_PANEL` | 14 | Panels, sheets |
| `R_PILL` | full | Status pills, avatars |

Zed sits at the tighter end (4–6); HIG at 6–10. 6 for anything you press, 10 for
anything that contains, 14 only for floating surfaces.

### 2.4 Colour tokens

Neutral ramp with a slight blue bias (a pure grey ramp reads dead next to a cool
accent).

```
bg.base      #0B0D10   window ground
bg.panel     #121519   sidebar, bottom panel
bg.raised    #191D23   row hover, card
bg.control   #20252C   button rest, field
sep          #262C35   1px hairline
sep.strong   #333B47   panel edges

label        #E8EBF0   primary
label2       #98A2B3   secondary
label3       #626C7C   tertiary, placeholders

accent       #7C8CFF   interactive: buttons, focus, selection
accent.press #6A79E8
on-accent    #0A0C12   text on accent fill
progress     #3BE8B0   "data in flight" only — never interactive
danger       #F87171   failures
```

Rule: **accent is only for things you can act on.** Progress bars, status dots
and success states use `progress`/`danger`, never `accent`. That single rule
fixes finding #6.

### 2.5 Elevation

macOS is flat. Depth comes from a surface step plus a hairline, not shadows.
One shadow exists in the whole system, on the drag-over overlay:

```
offset [0, 8]  blur 24  spread 0  color rgba(0,0,0,0.45)
```

### 2.6 Motion

| Transition | Duration | Curve |
|---|---|---|
| Hover / press | 90 ms | ease-out |
| Selection change | 140 ms | ease-out |
| Panel expand | 200 ms | ease-out-cubic |
| Drag-over | 160 ms | spring 2.0 / 0.70 |
| Progress bar | 250 ms | linear (data, not UI) |

No bounce anywhere except drag-over. Bounce in a utility window reads as a toy.

---

## 3. Component inventory

Everything below is painted, not an `egui::Widget`. Each has explicit states.

| Component | Height | States |
|---|---|---|
| `Row::device` | 52 | rest · hover · selected · focused |
| `Row::transfer` | 64 | active · done · failed · cancelled |
| `Button` | 28 / 32 | rest · hover · pressed · disabled · focused |
| `DropZone` | fill | rest · drag-over · busy · disabled |
| `Chip` | 24 | rest · interactive |
| `Progress` | 3px | determinate · indeterminate |
| `Divider` | 1px | full · inset `S3` |
| `EmptyState` | fill | default · with warning |
| `Banner` | 36 | info · warning · error |

Two that do not exist today and are the most visible gaps: **`Row::device`
selection** (the list is currently inert — clicking a device does nothing
visible) and **`Banner`** (discovery failures are currently a red line buried
inside the empty state).

---

## 4. Layout architecture

### 4.1 Shell

```
┌──────────────────────────────────────────────────┐
│ ●●●   ZenShare                  [ Kage's Mac ]   │  52  unified titlebar
├────────────────┬─────────────────────────────────┤
│ NEARBY      2  │                                 │
│ ┌────────────┐ │                                 │
│ │ Pixel 9    │ │           Drop zone             │  flexible
│ └────────────┘ │                                 │
│ ┌────────────┐ │                                 │
│ │ Studio     │ │                                 │
│ └────────────┘ │                                 │
├────────────────┴─────────────────────────────────┤
│ Transfers   2                              ⌄     │  36 → 220
└──────────────────────────────────────────────────┘
```

- **Titlebar**: `ViewportBuilder::with_fullsize_content_view(true)` (verified in
  egui 0.37, `viewport.rs:476`) so content extends under the traffic lights, with
  a 72px leading inset. Kills finding #8.
- **Sidebar**: 280 default, 240–360, draggable divider. Owns the device list.
- **Canvas**: the drop zone *is* the canvas — no card wrapping it. Kills #2.
- **Bottom panel**: collapsed 36, expanded 220, draggable. Transfers live here
  instead of overlapping the sidebar. Kills #4.
- **Status bar**: deleted. Peer count moves into the titlebar chip; the rest of
  its text was duplication. Kills #7.

### 4.2 Why a bottom panel and not a third column

Transfers are episodic — empty most of the time, and the thing you look at while
they run. A third column would be permanently empty; a collapsible bottom strip
costs 36px when idle and is the Zed/VS Code idiom people already know.

---

## 5. Refactor plan

Ordered so that every phase is independently shippable and independently
verifiable. **Phase 0 exists because of how this project has gone so far: the UI
refactor must be measured, not eyeballed.**

### Phase 0 — Snapshot harness (do this first)

`egui_kittest 0.37.0` is published and version-matched to our egui. It renders a
`Harness` headlessly and compares against checked-in PNGs.

- Four fixtures: empty, two peers, mid-transfer, error banner.
- Three sizes: 1080×700, 1440×900, 380×720 (narrow).
- Golden PNGs in `crates/zs-ui/tests/snapshots/`.
- CI job: `cargo test -p zs-ui`; update with `UPDATE_SNAPSHOTS=1`.

Risk to retire in the first hour: kittest's snapshot feature needs a GPU backend.
On the runner that means lavapipe. If it is flaky, fall back to Xvfb + `zenshare
--demo` + `import` — the same technique already used for `docs/screenshot.png`.
**Do not proceed past this phase until it is green in CI.**

### Phase 1 — Tokens

Rewrite `theme.rs` to the tables above. Add `TextStyle` entries to
`Style::text_styles` (verified: `style.rs:289`) so sizes are named, not literal.
No structural change. Snapshot diffs will be large and expected.

### Phase 2 — Primitives

Rebuild `widgets.rs` around the component table: rows, buttons, chips, dividers,
progress, banner. Every component takes a `Response` and derives its state from
it. This is where findings #5 and #6 get fixed.

### Phase 3 — Shell

Unified titlebar, sidebar, canvas, bottom panel. Delete `status_bar`. Split
`lib.rs` into `shell/`, `panels/`, `components/`. Kills #3, #4, #7, #8, #12.

### Phase 4 — Motion

`ctx.animate_bool_with_time` for hover/selection, `animate_value_with_time` for
panel heights. Per the motion table. No bounce outside drag-over.

### Phase 5 — Narrow and Android

The narrow layout currently planned (stacked panels) is replaced by: sidebar
collapses to a horizontal device strip above the canvas, bottom panel stays. The
380×720 snapshot fixture is the acceptance test.

### Phase 6 — Keyboard and accessibility

Tab order, focus ring (`accent`, 2px, 2px offset), Escape to cancel a transfer,
Enter to send. `egui_kittest` can drive focus, so this is testable too.

---

## 6. Two corrections to the brief

**There is no backdrop blur in egui.** "Glassmorphic blur" as macOS does it
requires sampling the framebuffer behind a rect, which needs a custom wgpu render
pass. That is possible but it is a renderer feature, not a style setting, and it
breaks the moment the backend changes. Recommendation: skip it. macOS HIG itself
favours solid materials for utility windows; translucency belongs on the
titlebar, and we can do that with `with_transparent` plus a solid fallback.

**`effectcraft-apple-motion` is not applicable.** It drives a video compositor
through an MCP server. Its spring and stagger constants are useful as reference
numbers (spring `2.0 / 0.70`, stagger 0.03–0.05s), and they informed the motion
table, but nothing in it produces UI code.

---

## Status

| Phase | State |
|---|---|
| 0 — Snapshot harness | **done.** Six fixtures, green in CI. |
| 1 — Tokens | **done.** `theme.rs` rewritten. |
| 2 — Primitives | **done.** `States`, `button`, `chip`, `divider`, `focus_ring`. |
| 3 — Shell | **done.** Unified titlebar, sidebar, canvas, bottom panel; status bar deleted. |
| 4 — Motion | not started. |
| 5 — Narrow / Android | **layout done.** File picking still needs SAF. |
| 6 — Keyboard and focus | not started. `focus_ring` exists but nothing draws it yet. |
