# LK67 Studio

A Windows control surface for the **GamaKay LK67 65% keyboard**: per-key RGB, remapping, macros and
music- or screen-reactive lighting. Rust + `eframe`/`egui`; no vendor software, SDK or driver needed.

## Quick start

```bash
cargo run --release
```

Windows 10/11, Rust 1.85+ (edition 2024).

1. **Driver & packets** → **Rescan HID devices**. The keyboard is matched by VID/PID
   (`0x3151:0x4015`). Dry-run is on by default, so nothing is written until you select the device
   and press **Use device**.
2. **Remap & keymap** → select a key and assign a key, combo, media key, macro, RGB command or
   profile action.
3. **Lighting** → pick one of the 13 hardware effects (or Custom / Music / Screen) and press
   **Write effect + palette**.

Settings save automatically to `%APPDATA%\LK67 Studio\config.json` (debounced, atomic write).

## Features

- **Profiles** — four independent profiles with their own keymaps, macros and lighting. Switch from
  the top bar, with `Ctrl+Alt+1..4`, or from the Driver page.
- **Remap** — 65% canvas with live LED colours; assign keys, combos, media keys, macros, RGB/profile
  actions, FN or Disabled, on both the normal and FN layers. Software remapping runs through
  `WH_KEYBOARD_LL` + `SendInput`, so it works in any Windows app without flashing firmware.
- **Knob** — the press is a normal remappable key (Play/Pause by default); rotation can send volume
  to Windows or drive LED brightness, animation speed and effect cycling. Its detents are HID
  consumer-control reports, which the app reads directly; LED modes cancel the OS volume step.
- **Macros** — record live or build steps (down, up, tap, combo, delay), set repeats or
  hold-to-repeat, bind to a key and preview playback on a cancellable thread.
- **RGB** — 13 hardware effects with a 14-slot palette, brightness, speed and direction; custom
  per-key painting, presets and keyframe animations; music-reactive (WASAPI loopback or mic, FFT)
  and screen-follow (GDI) modes. Effects preview in the app even in dry-run.
- **Key tester** — press counters, hold times, heatmap and a per-key log, fed by the hook and the
  app's own input.

## Architecture

```
src/
├── main.rs                 eframe entry point
├── app.rs                  shell: tick loop, events, save, hotkeys
├── model/                  keycodes, layout, actions, macros, lighting, profiles, store
├── driver/
│   ├── mod.rs              KeyboardDriver trait, HID discovery, dry-run log
│   └── lk67.rs             yc500 report IDs, packet builders, checksums
├── rgb/
│   ├── effects.rs          software renderer for all effects
│   ├── music.rs            WASAPI capture, FFT, beat detection
│   └── screen.rs           GDI capture and downsampling
├── input/
│   ├── inject.rs           SendInput helpers
│   ├── macro_player.rs     macro playback thread
│   └── hook.rs             keyboard hook, FN layer, hotkeys, remap dispatch
└── ui/                     theme, widgets, keymap canvas and one file per page
```

### Firmware protocol (ROYUAN yc500)

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

Verified on hardware: LEDPARAM readback after a static write, an Esc→A keymap write with readback
and restore, a paced USERPIC upload, and live `0x0D` band frames. The board drops the bulk keymap
command, so the app writes one slot at a time and waits ~550 ms for the control endpoint to settle.
`KEY_SLOTS` comes from the factory keymap (`slot = column * 6 + row`); the knob press is slot 75.

The knob press arrives as mute (`0xAD`) on this firmware although the layout models it as Play/Pause
(`0xB3`); both map to the Knob key.

### HID collections on Windows

A keyboard exposes several HID collections. Windows denies output writes on OS-claimed ones, and the
LK67's control collection (vendor interface 2, `0xFFFF/0002`) has a 65-byte feature report but no
output report, so packets are sent with `HID SetFeature`. Discovery marks each collection **VENDOR**
(writable), **INPUT-ONLY** or **READ-ONLY** and picks a writable vendor collection automatically;
**Use vendor** pins a specific one. Dry-run builds the same packets and logs them (hex + decoded
fields) in **Driver & packets**.

## Limitations

- **Tray icon** — closing the window hides it to the tray; quit from the tray menu.
- **Onboard keymaps are simple-action only.** Keys, media keys, FN and swallow can be stored on the
  board; macros, combos and app commands stay in software.
- **One remap path at a time.** While software remapping is on (the default), onboard keymap writes
  and firmware profile switching are skipped so a key is never remapped twice. Turn it off to use
  Driver → Write keymap. Driver → Clear onboard remaps restores stock entries.
- **Lighting edits apply immediately** (about 120 ms after the control settles), including on
  profile switches.
- **Live modes use the firmware's host-fed lighting**: music in modes 20/22 from `0x0D`, screen
  follow in mode 21 from `0x0E`. The board's rendering is simpler than the in-app preview.
- **Per-key pictures are flash-backed** and uploaded on demand from a worker thread; streaming them
  would stall the control endpoint.
- Software remapping, macros and reactive effects need the app running.
- Screen follow captures the primary monitor with GDI; music needs a render device or microphone.

## Troubleshooting

- `HID write failed ... 0x00000005` (access denied) or `0x00000001` — the selected collection has no
  writable report, or the transport is output. Pick a **VENDOR** row with a feature report
  (`0xFFFF/0002`, interface 2) and keep **Transport** on *Feature report*.
- `lk67-studio.exe --probe` prints the collection and interface the app opens.
- No device found — rescan and confirm the cable and VID/PID (`0x3151:0x4015`).

## Development

```bash
cargo test     # model, layout, storage, effects, macros
cargo check
cargo build --release
```
