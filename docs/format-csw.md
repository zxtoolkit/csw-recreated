# The CSW file format

**CSW** (Compressed Square Wave) stores the tape signal of an 8-bit home
computer as a run-length list of pulse durations rather than as audio samples.
A tape signal is really a 1-bit square wave, so the only information that
matters is *where the transitions are*, and storing just the gaps between them
is lossless.

The format was designed by Ramsoft, and **their specification is beside this
file**: [`csw.html`](csw.html) as they published it on 1 August 2003, with
[`csw.html.md`](csw.html.md) as a Markdown rendering of the same document for
reading on the web. Every header field, offset, type and value is there. It is
the authoritative source, and it is the file that shipped alongside the
MS-DOS converter, which is what its help screen means by "the enclosed
documentation".

This document does not reproduce it: no header tables, no field lists. It
records what can be observed about the format that the specification does not
cover — the model the header describes, and the questions it leaves to each
implementation — restating a field only where a note rests on it. It gives no
advice and describes no converter; what an implementation does with a file it
cannot read is not a property of the CSW format.

Two revisions exist and both are in use: **v1.01** (1999) and **v2.00** (2003).
v2.00 is the current revision; the specification still carries the v1.01 header
table beside it.

## The data model

A CSW file is three things:

1. a **sample rate**, in Hz — the clock the durations are counted in;
2. an **initial polarity** — whether the signal starts high or low;
3. a **sequence of pulse lengths**, each a positive integer number of samples.

The signal alternates on every pulse: if it starts high, pulse 1 is high, pulse
2 is low, pulse 3 is high, and so on. Nothing else is stored, and nothing else
is needed.

```
polarity = high        ┌─────┐     ┌───┐         ┌───────┐
                       │     │     │   │         │       │
                   ────┘     └─────┘   └─────────┘       └──
    pulse lengths:    3        5    1       4        7
```

**No amplitude information is stored**, because there is none to store — the
value is one of two states. A decoder reconstructing audio picks its own two
levels, conventionally the extremes of the sample format.

**Zero-length pulses do not exist.** The specification does not say so
outright; it follows from the encoding and from what a pulse is. `0x00` is the
escape byte introducing a 4-byte duration, so the one-byte form cannot express
a zero at all, and a pulse lasting no samples carries no information — levels
alternate, so all it would do is invert every level after it. The 4-byte form
can physically hold a zero. A file using it has no consistent reading: dropping
the pulse inverts every level that follows, and keeping it means a pulse of no
duration.

Reconstructing audio is: start at the initial polarity, emit that many samples
at one level, flip, repeat. Going the other way — sampled audio to CSW — means
deciding where the signal changes state and measuring the gaps between those
points. How that decision is made is not part of the format, and the
specification does not prescribe it. It is not necessarily a fixed threshold: a
detector may instead track direction changes with hysteresis scaled to the
signal, so that dither around the midpoint does not become a pulse train. Two
encoders can therefore return different pulse streams from the same noisy
recording, both conforming.

## What the specification leaves open

The specification addresses writers. Its only "must" statements are that
reserved bits be zero and that every header field be filled in, and it carries
no conformance language for readers at all. The gaps below are therefore gaps,
not omissions from this document; what fills them is a matter for each
implementation.

**Versions.** The specification defines 1.01 and 2.00 and gives no rule for
gating on them: nothing says whether a reader should refuse an unrecognised
revision outright, or read a hypothetical v2.99 as a v2 file on the grounds
that the layout it knows is the major revision's.

**Unknown compression types.** The enumeration is `1` (RLE) and `2` (Z-RLE,
marked "CSW v2.xx only"), and the enumeration is all there is; no behaviour is
specified for any other value, nor whether one should be read as damage or as a
revision the reader is too old to know. The two questions are the same one the
version field raises, and the format answers neither.

**Pulse data location.** In v2 the pulse data begins at `0x34 + HDR`, not at a
fixed `0x34`. The two coincide only while `HDR` is `0`, which is its current
default value.

**Sample rate.** Nothing constrains it beyond the field's width: a `DWORD` in
v2, a `WORD` in v1.01, which caps v1.01 at 65535 Hz. Nor is one rate
conventional: the two reference pairs in this repository were recorded at
32258 and 44100 Hz.

**Pulse magnitudes.** Pulses above 255 samples occur — a gap between blocks, or
a stretch of silence — and take the 4-byte escape. They are not universal:
neither reference pair in this repository contains one, their longest pulses
being 48 and 36 samples. Nor is the count small: the tone fixture runs at about
1000 pulses per second, so a three-minute recording is of the order of 180,000,
and the `DWORD` count admits 4,294,967,295.

**Round-tripping.** Decoding a CSW to samples and re-encoding at the same rate
returns the original pulse stream byte-for-byte under plain RLE — but only for
some renderings. Nothing in the format fixes the two levels a decoder renders
at, and an encoder has to detect where the signal changed state — for a
detector with hysteresis, that means the step between the rendered levels has
to clear its own. Rendered at the extremes the five-pulse example above
survives the round trip; rendered at 168 and 88 — a step of 80 of the 255
available, and an equally legal reconstruction — a detector wanting more comes
back with a single pulse. The format guarantees the round trip only when the
rendering and the detector agree, and it constrains neither.

## Relationship to other formats

CSW is a *signal* format: it records what the tape sounded like, not what the
data meant. TZX and TAP are *structured* formats, recording blocks, headers, and
bytes. Converting a recording to CSW is usually the first step, with pulse
detection into TZX as the second — CSW is the lossless intermediate that lets
the second step be retried without re-digitising the tape.

Because CSW discards nothing but amplitude, a CSW of a tape that will not
decode still holds everything the recording held bar its levels: what defeats a
loader today is preserved for whatever is tried next.
