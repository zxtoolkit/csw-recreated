# The OUT trace format

A `.OUT` file is a Z80 emulator's log of `OUT` writes to ZX Spectrum port
`0xFE`, made by a ROM or turbo tape loader while loading. Every border change
the loader makes on a detected tape edge is one edge of the reconstructed
square wave, so the trace is already a clean, noise-free pulse stream — no
sampling, no thresholding, nothing to filter.

Each record is 5 bytes, little-endian:

```
word_a = byte0 | (byte1 << 8)   // T-state offset within the current 1/200 s frame
word_b = byte2 | (byte3 << 8)   // the 16-bit I/O port, the Z80's BC
byte4                           // the value written
```

A tape loader's `OUT (254),A` puts the accumulator in the port's high byte, so
for a border write `byte3` and `byte4` hold the same value. A decoder that
wants edges needs neither: the signal is in the timing, and the level
alternates.

Decode straight to pulses, keeping `prev` (previous timestamp) and `accum`
(T-states accumulated for the current pulse), both starting at 0. For each
record:

* `word_a == 0xFFFE` → a marker the emulator writes for its own trace of the
  running program, carrying no timing; skip.
* `word_a == 0xFFFF` → the end of a 1/200 s frame, where `word_b` is that
  frame's length in T-states — 17472 on a 48K machine, 17727 on a 128K one:
  `accum += word_b - prev`, then `prev = 0`. The frame markers are what make
  the per-frame offsets add up into a continuous clock.
* otherwise, if `(word_b & 0xFF) == 0xFE` → a write to port `0xFE`, ending a
  pulse: `accum += word_a - prev`; `prev = word_a`; emit a pulse of
  `accum * rate / 3_500_000` samples if that is greater than zero; then
  `accum = 0`. Whether a fractional length is truncated or rounded is the
  consumer's choice (see below); truncating drops a sub-sample pulse, and its
  edge with it.
* otherwise → a write to some other port (AY sound, say); ignore it.

Nothing in the format requires the timestamps to run forward, and nothing says
what to do when they do not. The MS-DOS converter holds its clock and its
accumulator in 32 bits and lets them wrap, so a timestamp that steps backwards
does not produce a negative gap — it produces a very large positive one, and
the pulse it converts to is written out at that length. A trace stepping back
1000 T-states yields a single pulse of 0x04C118CB samples, some twenty minutes
of one level at 65000 Hz.

That is worth knowing in both directions. A reader that reproduces it stays
compatible; a reader that checks instead should say the timestamps do not run
forward, rather than emit the pulse and leave the twenty minutes to be
explained later. What it must not do is truncate the length into a DWORD it no
longer fits, which can wrap it to zero — a zero-length pulse sends the reader
looking in quite the wrong place.

A file with no port-`0xFE` record at all carries no signal and should be
rejected rather than decoded to silence — in practice it means the emulator was
logging a different port, or none.

A trace carries no sample rate: it is a list of T-state intervals against the
Spectrum's 3.5 MHz clock. Three choices are the consumer's alone — the rate,
which level the waveform opens at, and whether a pulse length that lands
between samples is truncated or rounded — so two readers can turn one trace
into two different CSW files without either being wrong about the format.
