# Reference fixtures

Each pair here is a WAV and the CSW its pulse stream must encode to. The CSW
files were written by the MS-DOS converter, not by this repository; the
encoder here is checked against them.

| Pair | Covers |
|------|--------|
| `tone-8bit.wav` / `.csw` | the detector's adaptive hysteresis: the deadband a reversal must clear, and the number of samples it must hold for |
| `square-8bit.wav` / `.csw` | its large-jump path, where a step of more than half the 8-bit range is an edge on its own |

Each pair is exact: the CSW's pulse lengths sum to the WAV's frame count.

`tests/corpus.rs` asserts that encoding the WAV reproduces the reference pulse
stream exactly, and two tests of its own go further: each WAV re-encodes to a
file byte-identical to its reference CSW, deflate stream included — all 612
bytes of `tone-8bit.csw` and all 76 of `square-8bit.csw`.

The tone pair is the more sensitive of the two. Its 2029-byte RLE stream packs
to 560 bytes, enough that a change of deflate level, window bits or strategy
moves them; the square's 1080 bytes pack to 24, which a strategy change leaves
alone. Neither is long enough to notice the memory level, which only shows past
16384 literals.

## What the tone signal is, and what it is not

`tone-8bit.wav` is a synthetic tone captured through a soundcard input. It is
**half-wave rectified** — samples span 127..192 and never dip below the
midpoint — with a fundamental near 504 Hz. That asymmetry is useful here,
since it is exactly what separates a fixed threshold from the adaptive
detector. But neither fixture is real tape audio, and both are clean.

## Known limitations

The strongest check on this encoder is not in this directory and cannot be
distributed with it: a full-length recording of real tape audio. What ships
here is what a reader can run.

Two gaps go untested by the pairs above:

* **A detector stage neither pair reaches.** Alongside the hysteresis, the
  pulse detector keeps a short history of recent pulses which, when the
  `flagged` bit derived from it is set, can bypass the hysteresis and emit
  directly.
* **The rejection branch goes unexercised.** The gate that discards a reversal
  whose movement is too small a share of the previous pulse's amplitude
  is evaluated on every emit and passes on every signal tried so far.
