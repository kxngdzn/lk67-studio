# LK67 Studio

A Windows desktop control surface for the **LK67 65% mechanical keyboard**: per-key RGB lighting,
remapping, macros, music-reactive and screen-follow effects — written in Rust with `eframe`/`egui`.

The app talks to the keyboard over HID through a `KeyboardDriver` abstraction that ships with a
**Dry-Run / Simulation mode**, so every screen, packet and effect can be exercised before the real
firmware report IDs are confirmed.

---

## Quick start

```bash
cargo run --release
```

Requirements: Windows 10/11, Rust 1.85+ (edition 2024). No SDK, driver or vendor software is needed.

First launch:

1. **Driver & packets** → *Rescan HID devices*. The LK67 interface is matched by VID/PID
   (`0x3151:0x4015`, GamaKay LK67, by default). Dry-run is enabled by default, so nothing is written to hardware.
2. Pick your keyboard in the discovery list and press **Use device** to switch to live mode.
3. **Remap & keymap** → select a key → assign a key, combo, media key, macro or RGB/profile action.
4. **Lighting** → pick one of the 13 hardware effects (or Custom, Music, Screen) and press
   **Write effect + palette**.

All settings are saved automatically to `%APPDATA%\LK67 Studio\config.json`.

---

## Features

### Profiles
* Four independent profiles, each with its own normal/FN keymaps, macros and lighting.
* Switch from the top bar, with `Ctrl+Alt+1..4` global hotkeys, or send the profile packet to the
  firmware (Driver page).
* Debounced JSON persistence with atomic temp-file + rename writes.

### Remap (normal + FN layer)
* Click-to-select 65% canvas with live per-key LED colours and pressed-key highlights.
* Assign another key, a **multi-key combo** (`Ctrl+Shift+Esc`), a media key, a macro, an RGB command,
  a profile switch, the FN modifier, or *Disabled* (swallowed).
* Base vs. remapped labels are shown on each keycap; the FN-layer view shows FN-specific hints.
* Per-key reset, per-layer reset and full-profile reset.
* Optional FN key (Settings) switches to the FN layer while held (e.g. Caps Lock).
* Volume knob card: the knob press is a normal remappable key (Play/Pause by default, not mute), and
  turning the knob can send volume keys to Windows or drive LED brightness, animation speed or
  effect cycling.
* Remapping is performed in software by a low-level keyboard hook (`WH_KEYBOARD_LL`) with
  `SendInput`, so it works on any Windows app without flashing firmware.

### Macros
* Live recording of press/release order, or hand-built steps (down, up, tap, combo, delay).
* Per-step delays plus a default delay for recorded steps.
* Repeat once, *N* times, toggle on/off, or hold-to-repeat while the key is held.
* Bind to a key or combo, preview playback, and stop with Escape / the Stop button.
* Playback runs on its own cancellable thread; macro events are tagged so the hook does not re-map
  its own injected keystrokes.

### RGB lighting
* **13 hardware effects**: Off, Static, Breathing, Rainbow wave, Rainbow cycle, Colour wave, Snake,
  Reactive ripple, Reactive sparkle, Marquee, Twinkle, Raindrop, Fireworks — with the 14-slot palette,
  brightness, speed and direction controls.
* **Custom per-key layouts**: paint keys or whole rows with the palette or the colour picker, clear
  keys, save/load presets.
* **Animated custom layouts**: capture keyframes from the painted layout or from a live effect
  preview, set FPS, playback speed and direction, loop the animation.
* **Music reactive**: WASAPI loopback (system audio) or microphone capture, 2048-point FFT,
  16 logarithmic bands, beat detection, four visual styles (Equalizer, Pulse, Wave, Twinkle),
  sensitivity and smoothing.
* **Screen follow (Ambilight)**: GDI screen capture at 5–30 FPS, grid downsampling and zone colours
  with smoothing and saturation controls, plus a live preview in the app.
* Effects render in the 65% canvas preview even in dry-run; streaming pushes the per-key frame buffer
  to the driver at the configured FPS.

### Key tester
* Real-time switch tester: physically press keys and they light up on the 65% layout, with press
  counters, hold times, a heatmap and a per-key log.
* Presses come from the Windows hook; keys typed while the window has focus are also read from the
  app's own input, so the tester keeps counting if the hook is unavailable.

---

## Architecture

```
src/
├── main.rs                 eframe entry point
├── app.rs                  application shell: tick loop, event/command drain, save, hotkeys
├── model/
│   ├── keycode.rs          virtual-key table, names, serialisation, key picker data
│   ├── layout.rs           68-key 65% ANSI layout, LED indices, matrix placeholders
│   ├── action.rs           Key / Combo / Macro / Media / FN / RGB / Profile / Disabled actions
│   ├── macro_def.rs        macro steps and repeat modes
│   ├── lighting.rs         13 effect definitions, palette, custom layers, presets, configs
│   ├── profile.rs          profile + settings structs
│   └── store.rs            Config, JSON load/save, validation
├── driver/
│   ├── mod.rs              KeyboardDriver trait, Lk67Driver, HID discovery, dry-run log
│   └── lk67.rs             ALL LK67 protocol knowledge: report IDs, packet builders, checksums
├── rgb/
│   ├── effects.rs          software renderer for all 13 effects, custom, music and screen modes
│   ├── music.rs            WASAPI capture thread, FFT analysis, beat detection
│   └── screen.rs           GDI capture thread and downsampling
├── input/
│   ├── inject.rs           SendInput helpers
│   ├── macro_player.rs     macro playback thread
│   └── hook.rs             WH_KEYBOARD_LL hook, FN layer, hotkeys, remap dispatch
└── ui/
    ├── theme.rs            dark navy/teal styling
    ├── widgets.rs          key picker, palette, sliders, meters, previews
    ├── keymap.rs           interactive 65% canvas
    └── remap.rs, lighting.rs, macros.rs, music.rs, screen.rs, tester.rs, driver.rs, settings.rs
```

### Firmware protocol (ROYUAN yc500)

The LK67 uses the ROYUAN **yc500** command set on its vendor feature-report collection:

| Item | Value |
| --- | --- |
| Report ID | `0` (64 data bytes, 65-byte feature report) |
| Opcode | data byte 0 |
| Checksum | `0xFF - sum(data[0..at])`; `at = 7` normally, `8` for LEDPARAM |
| Lighting | GET `0x87`, SET `0x07` |
| Keymap read | GET `0x89`, 8 pages of 64 bytes |
| Keymap write | single slot `0x13` base / `0x15` FN, profile in byte 1, slot in byte 2, value in bytes 8–11 |
| Per-key colours | USERPIC `0x0C`, 7 pages of 56 bytes, shown by LEDPARAM mode 13 |
| Music stream | `0x0D`, 42 band heights (0–6) at ~50 Hz, LEDPARAM mode 20/22 |
| Screen colour | `0x0E`, one RGB colour for the board, LEDPARAM mode 21 |
| Profile select | `0x05` |

The knob press reaches Windows as mute (`VK 0xAD`) on this firmware even though
the layout models it as Play/Pause (`VK 0xB3`); the app maps both codes to the
Knob key and defaults it to Play/Pause, and the Volume knob card can turn the
knob's volume keys into LED brightness, speed or effect control.

Verified on hardware: LEDPARAM readback after a static write, an Esc→A keymap
write followed by readback and restore, a paced 7-page USERPIC upload that
left the board in mode 13, and mode 20 accepting live `0x0D` band frames.
The LK67 drops the 9-page bulk keymap
command, so the app uses the vendor “simple” single-slot writes and waits ~550 ms
per changed key for the control endpoint to settle. `KEY_SLOTS` comes from the
board's own factory keymap (`slot = column * 6 + row`); the rotary knob press is
slot 75.

All device-specific constants and builders remain in `src/driver/lk67.rs`.

With dry-run enabled, `Lk67Driver` builds the exact same packets and logs them in
the **Driver & packets** page without opening a write handle.

### HID collections on Windows

A keyboard exposes several HID collections. Windows blocks output reports on the standard (OS-claimed)
collections, so `WriteFile` fails with `ERROR_ACCESS_DENIED (0x00000005)`. The LK67's control collection
is the vendor interface (usually interface 2, usage `0xFFFF/0002`) — and it declares a **65-byte feature
report with no output report at all** (verified with `HidP_GetCaps`: `out = 0`, `feature = 65`).

Packets are therefore sent with `HID SetFeature` ("Feature report" transport, the default). A read of
feature report `0x0B` on the vendor interface returns the current effect data, confirming the transport.

The app queries each vendor collection's report capabilities and marks it **VENDOR** (has a writable
report), **INPUT-ONLY** (e.g. `0xFFFF/0001`, 4-byte input, no writable report) or **READ-ONLY** in
*Driver & packets → HID discovery*. It automatically picks a writable vendor collection, preferring the
dedicated interface. A saved usage page is only honoured if that collection actually has a writable
report. Use **Use vendor** (or click **Use** on a VENDOR row) to pin a specific collection.

With dry-run enabled, `Lk67Driver` builds the exact same packets it would send and logs them in the
**Driver & packets** page (hex dump + decoded fields), so the keymap, colour and effect pipelines can
be validated first. To finalise the protocol: capture the vendor software with Wireshark + USBPcap,
update the constants/packet builders in `driver/lk67.rs`, then switch dry-run off.

---

## Storage

`%APPDATA%\LK67 Studio\config.json` — includes driver configuration, four profiles, the active
profile, settings and UI preferences. Saves are debounced (~1 s) and written via a temporary file.

## Limitations

* **Tray icon.** The app keeps a tray icon with Show, Profile 1..4 and Quit. Closing the window
  hides it to the tray instead of quitting; use Quit in the tray menu to exit. If the platform
  refuses the icon, the app falls back to closing normally.
* **Onboard keymaps are simple-action only.** The LK67 can store keys, media keys, FN and swallow;
  macros, combos and app commands stay in software.
* **One remap path at a time.** While software remapping is on (the default), the app skips onboard
  keymap writes and does not switch firmware profile slots, so a key is never remapped by both the
  board and the hook. Turn software remapping off to use Driver → Write keymap. Driver → Clear
  onboard remaps reads all four firmware slots and restores stock entries, removing swaps written
  by earlier builds.
* **Lighting edits apply immediately.** Effect, colour, brightness, speed and direction changes are
  written as soon as the control settles (about 120 ms), with no Apply click needed. Profile
  switches also send the new lighting right away.
* **Live modes use the firmware's own host-fed lighting.** Music runs in the board's mode 20
  (Classic) or 22 (EDM) from `0x0D` band frames, and screen follow runs in mode 21 from `0x0E`
  colour frames, both at about 20–50 Hz with no flash writes. The keyboard draws music as its own
  bars and screen follow as one blended colour for the whole board; the in-app grid is a richer
  preview than the board can show live.
* **Per-key pictures are flash-backed.** A painted layout is uploaded as seven `0x0C` pages with
  100 ms spacing and a 2 s settle, then shown by mode 13. The app sends it on **Send per-key
  picture** from a worker thread, so the UI never blocks; uploading frames continuously would
  stall the board's control endpoint until it is replugged.
* **Reactive effects use the palette.** LEDPARAM carries the selected colour with the fixed-colour
  flag for every effect except Rainbow wave/cycle, so ripple, sparkle, twinkle, raindrop and
  fireworks no longer fall back to the firmware's stock preset colour.
* Software remapping, macros and reactive effects require the app to stay running.
* Screen follow captures the primary monitor with GDI; multi-monitor composition is not implemented.
* Music reactive mode needs an active Windows render device for loopback, or a microphone the app is
  allowed to access.
* The 68-key layout is the standard ANSI 65% arrangement; the LK67's real matrix order is mapped
  in `KEY_SLOTS` in `driver/lk67.rs`.

## Troubleshooting

* `HID write failed: ... (0x00000005)` / "Access denied" or `(0x00000001)` / "Incorrect function" —
  the selected collection has no writable report (standard collection, or input-only vendor
  `0xFFFF/0001`) or the transport is output. Pick a row marked **VENDOR** with a feature report
  (`0xFFFF/0002`, interface 2) and keep **Transport** on *Feature report*.
* Run `lk67-studio.exe --probe` in a terminal to print which collection and interface the app opens.
* No device found — press **Rescan / reconnect**; confirm the keyboard is wired and the VID/PID match
  (`0x3151:0x4015` for the GamaKay LK67).

## Development

```bash
cargo test     # model, layout, storage, effects, macros
cargo check
cargo build --release
```
