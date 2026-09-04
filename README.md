# csw

A command-line converter between sampled tape audio and the **CSW (Compressed
Square Wave)** tape-image format: a recreation in Rust of Ramsoft's MS-DOS
converter, writing the same bytes it wrote. It encodes WAV, VOC, IFF/8SVX and
Z80-emulator port traces (OUT) to CSW, and decodes CSW back to WAV or VOC.

`docs/` holds the CSW [specification](docs/csw.html.md) and the [MakeTZX
manual](docs/mtzxman.htm.md) as published and in Markdown, and this
repository's own notes on the [CSW](docs/format-csw.md) and
[OUT](docs/format-out.md) formats.

## Install

Each release attaches Windows, a universal macOS build, and Linux x86-64 and
aarch64, each needing nothing installed. Every archive holds the program,
`csw.html` and its diagram; `SHA256SUMS` sits beside them, with a
build-provenance attestation on public releases.

On macOS the extracted program arrives quarantined by Gatekeeper, which
refuses to run it on a double-click. Open it once from Finder with
right-click → Open, or clear the flag with `xattr -c csw`.

## Build

Rust 1.87 or later, and a C compiler — zlib is vendored and compiled from
source rather than linked from the system.

```sh
cargo build --release       # binary at target/release/csw (.exe on Windows)
cargo install --path .      # or install into ~/.cargo/bin
cargo test                  # unit + end-to-end tests
```

`--no-default-features` drops `zlib-c`, the vendored C zlib the Z-RLE writer
compresses through: that build needs no C compiler, reads both compressions,
writes the plain-RLE and v1.01 outputs, and refuses the default Z-RLE one,
having emptied the output file first.

```sh
cargo build --release --no-default-features
```

A Linux binary that needs nothing installed at all:

```sh
rustup target add x86_64-unknown-linux-musl
sudo apt-get install -y musl-tools    # a C compiler for the target
cargo build --release --target x86_64-unknown-linux-musl
```

## Usage

```
csw [options] inputfile [outputfile]
```

The switches are the MS-DOS converter's, and `csw -?` reproduces its help
screen below the banner: it says "decompress" for decode and "SoundBlaster
compatibility mode" for `-c`, omits `-1`, `-z` and `-i`, and lists every `-f`
sub-option, `-f3` (3DNow!) included, which is accepted and ignored. (`csw
'-?'` in zsh, which globs the `?`; `csw` alone shows it too.)

| Option | Effect |
|--------|--------|
| `-d`, `-dv` | Decode CSW to WAV, or to VOC (implied when the input extension is `.csw`) |
| `-k`   | Empties a file: it names a WAV from the *output*, creates it empty, and reads that in place of the input |
| `-1`   | Write a CSW v1.01 file (plain RLE) |
| `-z`   | Write plain RLE instead of the default Z-RLE |
| `-f[...]` | Run the digital filter before pulse detection |

`-r`, `-s<rate>`, `-t<secs>`, `-c` and `-i<n>` are the MS-DOS converter's
soundcard switches. They parse, and this build has no soundcard input: `-r`
says so and converts nothing.

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
```

[`fixtures/`](fixtures/README.md) holds `tone-8bit.wav`, `square-8bit.wav`
and their CSWs — the WAV encode and the decodes to WAV and to VOC; there is
no VOC or OUT input fixture.

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

The `CSW v2.00` stamp written into a CSW v2 header is that program's own
text, reproduced verbatim so that files can be compared byte-for-byte. The
banner — `-=[ CSW v2.00 ]=-  Ramsoft's CSW converter, recreated (GPL v2.0+).`
— is this tool's own, keeping the shape of the line it replaces. Neither is a
claim of authorship or a trade mark, and if Ramsoft would prefer not to be
named, the references will be removed on request.

## License

Copyright (C) 2026 AJ Banck.

The source in this repository — the Rust crate, the documents and the
fixtures — is licensed under the GNU General Public License, version 2 or (at
your option) any later version. See [`LICENSE`](LICENSE).
