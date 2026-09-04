# csw

A command-line converter between sampled tape audio and the **CSW (Compressed
Square Wave)** tape-image format: a recreation in Rust of Ramsoft's MS-DOS
converter, writing the same bytes it wrote. It encodes WAV, VOC, IFF/8SVX and
Z80-emulator port traces (OUT) to CSW, decodes CSW back to WAV or VOC, and
records straight from a soundcard input.

`docs/` holds the CSW [specification](docs/csw.html.md) and the [MakeTZX
manual](docs/mtzxman.htm.md) as published and in Markdown, and this
repository's own notes on the [CSW](docs/format-csw.md) and
[OUT](docs/format-out.md) formats.

## Install

Each release attaches Windows, a universal macOS build, and Linux x86-64 and
aarch64, the Linux pair each as a static archive needing nothing installed
and a `-glibc` one needing glibc ≥ 2.35 and ALSA — the archive to take to
record with `-r` where a sound server holds the card ([Audio
backends](#audio-backends)). Every archive holds the program, `csw.html` and
its diagram; `SHA256SUMS` sits beside them, with a build-provenance
attestation on public releases.

On macOS the extracted program arrives quarantined by Gatekeeper, which
refuses to run it on a double-click. Open it once from Finder with
right-click → Open, or clear the flag with `xattr -c csw`.

## Build

Rust 1.87 or later, and a C compiler — zlib is vendored and compiled from
source rather than linked from the system. DirectMode is on by default and
needs ALSA's headers on Linux (`libasound2-dev`), nothing extra elsewhere.

```sh
cargo build --release       # binary at target/release/csw (.exe on Windows)
cargo install --path .      # or install into ~/.cargo/bin
cargo test                  # unit + end-to-end tests
```

`--no-default-features` drops both the audio stack and `zlib-c`, the vendored
C zlib the Z-RLE writer compresses through. `-r` then reports itself
disabled, and the build needs no C compiler: it reads both compressions,
writes the plain-RLE and v1.01 outputs, and refuses the default Z-RLE one,
having emptied the output file first. Add back what you want:

```sh
cargo build --release --no-default-features --features zlib-c
```

A Linux binary that needs nothing installed at all, recording through the
kernel rather than through an audio library:

```sh
rustup target add x86_64-unknown-linux-musl
sudo apt-get install -y musl-tools    # a C compiler for the target
cargo build --release --no-default-features --features record-alsa,zlib-c \
  --target x86_64-unknown-linux-musl
```

## Usage

```
csw [options] inputfile [outputfile]
csw -r [options] name [outputfile]   # record from the soundcard
```

The switches are the MS-DOS converter's, and `csw -?` reproduces its help
screen below the banner: it says "decompress" for decode and "SoundBlaster
compatibility mode" for `-c`, omits `-1`, `-z` and `-i`, and lists every `-f`
sub-option, `-f3` (3DNow!) included, which is accepted and ignored. (`csw
'-?'` in zsh, which globs the `?`; `csw` alone shows it too.)

| Option | Effect |
|--------|--------|
| `-d`, `-dv` | Decode CSW to WAV, or to VOC (implied when the input extension is `.csw`) |
| `-r`   | DirectMode: record from the default audio input instead of reading a file |
| `-s<rate>` | Sampling rate for `-r` in Hz (default 32258); a sampled input uses its own, and OUT is rendered at 65000 Hz |
| `-t<secs>` | Stop a recording after `<secs>` seconds' worth of samples at the **requested** rate |
| `-k`   | Also keep the recorded samples as an 8-bit WAV beside the CSW. Without `-r` it empties a file: it names the WAV from the *output*, creates it empty, and reads that in place of the input |
| `-c`   | SoundBlaster compatibility mode: take the device's own configuration unchanged |
| `-1`   | Write a CSW v1.01 file (plain RLE) |
| `-z`   | Write plain RLE instead of the default Z-RLE |
| `-i<n>` | Accepted and ignored; it selected a recording input on hardware this tool does not drive |
| `-f[...]` | Run the digital filter before pulse detection |

The output name is the input cut at its **first** dot plus `.csw` (`.wav` or
`.voc` on a decode), so a `w8.wav` inside a directory named `a.b` writes
`a.csw` beside that directory. Name the output when the path carries a dot
before the file's own, and on a decode: without a name, `csw tone-8bit.csw`
writes over `tone-8bit.wav`.

Spelled `csw` here, as after `cargo install`. Writing, the messages call
plain RLE "the old compression method"; reading, the header line numbers the
compression — 1 for plain RLE, 2 for Z-RLE.

```sh
csw tape.wav                 # encode WAV -> tape.csw (CSW v2, Z-RLE)
csw tape.voc tape.csw        # encode a VOC recording
csw game.out                 # convert an emulator port trace (fixed 65000 Hz)
csw tape.csw out.wav         # decode -> out.wav
csw -dv tape.csw out.voc     # decode -> out.voc
csw noisy.wav -f             # encode with the default band-pass filter
csw -r tape                  # record from the soundcard -> tape.csw
csw -r -t60 -k tape          # record one minute, keep tape.wav as well
```

[`fixtures/`](fixtures/README.md) holds `tone-8bit.wav`, `square-8bit.wav`
and their CSWs — the WAV encode and the decodes to WAV and to VOC; there is
no VOC or OUT input fixture.

### DirectMode (`-r`)

The name on the command line is the one the output is *derived* from, exactly
as an input file name is: `csw -r rec.dat` writes `rec.csw`, and a second name
is taken as the output as it stands. A first name ending in `.csw` means
"decode", DirectMode included, so `csw -r tape.csw` takes the decode path —
but against DirectMode's own fixed input name, so it fails with `ERROR: Input
file 'csw00000.raw.csw' not found or invalid file type.`, leaving an empty
`csw00000.raw` and an emptied `tape.wav` behind. Spell it `csw -r tape`,
which writes `tape.csw`.

A volume meter runs first so the input level can be set: a green bar for the
RMS level of the last buffer, a red one for the share of it that hit the
rails. Any key starts recording — except **Ctrl-C**. During the recording,
**`P`** pauses and **any other key stops**; once there are samples, Ctrl-C
stops rather than aborts. `-t<secs>` stops automatically after the given time,
and is also what allows `-r` to run without a terminal.

#### Audio backends

DirectMode reaches the host through one of two backends, chosen when the crate
is built. They record identically and differ only in how they reach the
hardware.

**`record`** (default, every platform) uses `cpal`, over CoreAudio, WASAPI or
ALSA. It records from whichever input the host calls the default, so the
device is chosen in the operating system's sound settings, and a sound server
that is already running is what it goes through.

**`record-alsa`** (Linux only, and the one a static build uses) opens
`/dev/snd/pcmC<card>D<device>c` and drives the kernel directly: the card is
taken exclusively, at its own rate and format, with nothing resampling in
between. It takes the lowest-numbered capture device that will open, and
`CSW_ALSA_DEVICE` overrides that — `hw:1,0`, `1,0`, `1`, or a path under
`/dev/snd`. If a sound server already holds the card, the open fails with
`EBUSY`.

### Digital filter (`-f`)

`-f` runs an IIR filter over the samples before pulse detection, with the
MS-DOS converter's sub-options and defaults — single letters, one per `-f`
switch, so `-fo4 -fh5000 -fl500` rather than `-fo4h5000l500`. It shipped in
MakeTZX, Ramsoft's TZX converter, whose
[manual](docs/mtzxman.htm.md#11-the-digital-filter) gives their own hints: a
low-pass for hiss, the default band-pass for DC offset and 50 Hz hum, orders
above 4 rarely worth it, and cut-offs moved gradually from the default
rather than guessed.

```sh
csw tape.wav -f              # the default band-pass
csw tape.wav -f -ft3         # low-pass instead, for hiss
csw tape.wav -f -fo4         # a steeper filter
```

## Acknowledgements

The CSW format was designed by Ramsoft, whose MS-DOS converter established
the conversion workflow this tool follows. This is an independent
implementation, not affiliated with or endorsed by them.

The banner this tool prints, and the `CSW v2.00` stamp it writes into a CSW
v2 header, are that program's own text, reproduced verbatim so that console
output and written files can be compared byte-for-byte against it. The
copyright the banner states is Ramsoft's, left intact for that reason.
Neither is a claim of authorship or a trade mark, and if Ramsoft would prefer
that this tool not reproduce them, they will be removed on request.

## License

Copyright (C) 2026 AJ Banck.

The source in this repository — the Rust crate, the documents and the
fixtures — is licensed under the GNU General Public License, version 2 or (at
your option) any later version. See [`LICENSE`](LICENSE).

One caveat applies to binaries, not to the source. The default `record`
backend links [`cpal`](https://github.com/RustAudio/cpal), which is
Apache-2.0-only; that license is compatible with GPL-3 but not with GPL-2. A
binary built with it — the macOS, Windows and Linux `-glibc` release assets —
is therefore distributed under the terms of GPL-3, whose text is at
<https://www.gnu.org/licenses/gpl-3.0.txt>. Builds without `cpal` (the
static Linux assets, built with `record-alsa`, and any
`--no-default-features` build) carry no such component and may be
distributed under GPL-2.
