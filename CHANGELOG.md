# Changelog

## 0.2.0 - 2026-11-27

### New features

- **Direct knob rotation input.** The app now reads the knob's detents from the keyboard's HID
  consumer-control collection instead of relying on the low-level keyboard hook, which Windows
  never lets see them. Rotation drives Windows volume or the LED brightness, speed and effect
  cycling, and the volume step is cancelled in the LED modes.
- **`--knob-probe`.** A diagnostic mode that logs every consumer and vendor input report from the
  keyboard for five minutes to `knobprobe.log` in the config folder.
- **Firmware-accurate effect preview.** The 13 hardware effects now render from the yc500
  firmware's model: a 5 ms frame clock, the real 128-slot LED matrix, per-effect timing and
  brightness tables, and the seven-colour preset palette. Random effects (ripple, stars, snowfall,
  meteor) remain deterministic approximations because the firmware seeds are unreadable.

### Bug fixes

- The colour and rainbow waves now travel left to right for **Left to right** and right to left for
  **Right to left**; the two directions were swapped in the preview.
- The in-app keyboard now places the knob in the top-right corner above **Del**, matching the
  physical LK67 instead of the stock 65% End position.
- The preview addresses the same LED slots as the board (including the knob at slot 75) and uses
  the firmware's five brightness and speed levels.
- Brightness and speed move 25 points per detent, so a click always crosses a firmware level
  instead of being swallowed by rounding.
- Knob rotation for LED modes no longer leaves a Windows volume step behind.

### Internal

- Removed the hook-based knob-turn command path; `KnobTurn` handling now lives in the app's HID
  reader.
