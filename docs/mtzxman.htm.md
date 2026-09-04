# MakeTZX 2 - User's guide

![MakeTZX 2](mtzx2.gif)

**User's Guide**

**Version 2.33 (August 1st 2003)**
Latest manual revision: 01/08/2003

## SUMMARY:

1. [Introduction](#1-introduction)
2. [System requirements](#2-system-requirements)
3. [Features list](#3-features-list)
4. [Soundcard DirectMode (realtime)](#4-soundcard-directmode-realtime)
5. [Command line parameters](#5-command-line-parameters)
6. [Understanding MakeTZX's messages](#6-understanding-maketzxs-messages)
7. [How to identify a custom loader](#7-how-to-identify-a-custom-loader)
8. [The CSW file format](#8-the-csw-file-format)
9. [How to use OUT files](#9-how-to-use-out-files)
10. [How to convert](#10-how-to-convert)
11. [The digital filter](#11-the-digital-filter)
12. [Graphic interface: MakeTZX WinGUI](#12-graphic-interface-maketzx-wingui)
13. [FAQs (Frequently Asked Questions)](#13-faq-frequently-asked-questions)
14. [Revision history](#14-revision-history)
15. [Known limits and bugs](#15-known-limits-and-bugs)
16. [Future enhancements](#16-future-enhancements)
17. [Credits and contact info](#17-credits-and-contact-info)

---

## 1. Introduction

MakeTZX is a TZX creation utility that will help you to recover your old Spectrum tapes and preserve them from the damages of time. If you don't know what TZX files are, [read this](#12-what-are-tzx-files) first.

It works with the most common sample file formats, or directly from the soundcard's input **in true realtime** (see [DirectMode](#4-soundcard-directmode-realtime) later).

MakeTZX is based on a powerful decoding engine, easy to use and extremely accurate at the same time. This tool was developed with efficiency in mind, that is we wanted something that worked for real, without having to make tenths of retries before getting a successful conversion. Today MakeTZX converts the 97% of sample files at the first attempt, thanks to an intelligent decoder which can figure out lots of fine-tuning internal settings and is able to correct many signal distortions and errors automatically.

Besides, MakeTZX now has an amazing feature that guarantees **100% failure-proof conversions** even in the most difficult cases, simply the best result that you may ask to a TZX decoder! Read the [OUT files](#9-how-to-use-out-files) section to learn more.

We strongly recommend you to read this manual carefully. It contains lots of explanations and suggestions that are essential to discover all MakeTZX's capabilities and to get the most from it. Don't forget the [command line](#5-command-line-parameters) description where some important details are explained.

If you are experiencing problems with MakeTZX, please be sure you have read the [FAQ](#13-faq-frequently-asked-questions), [Known limits and bugs](#15-known-limits-and-bugs) and the [How to convert](#10-how-to-convert) sections, first!

---

## 2. System requirements

To run MakeTZX you will need at least:

#### Windows version:

- A 386 or better CPU.
- Windows 95/98/ME and above, or Windows NT 3.51 and above.
- Any soundcard with drivers installed if you want to use realtime conversion (DirectMode).

#### MSDOS version:

- A 386 or better CPU, any speed. Z80 is obviousely better but not yet supported :-)
- MSDOS 3.3+ with a DPMI host (such as CWSDPMI.EXE). For long filenames support, you must have Windows 9x.
- For realtime conversion (DirectMode), it is required an ISA SoundBlaster (or 100% compatible) with DSP 2.00+ or a Windows Sound System compatible soundcard (Crystal/Analog codec like CS4231, AD1848 and compatibles). Read the [DirectMode](#4-soundcard-directmode-realtime) section carefully for some compatibility issues). Note that modern PCI soundcards usually fail at sampling in SB legacy mode.

#### Linux version:

- Any Linux x86 distribution should be ok; tested on Slakware 3.4 - 3.5, Red Hat 5.2 - 7.1 and Mandrake 7.2.
  Recommended kernel 2.2.xx and above.
- For realtime conversion (DirectMode), make sure that your kernel is configured for sound support (built-in or through modules). MakeTZX uses the Open Sound System which comes with nearly all Linuxes (OSS/Free); recommended v3.6 and above.

#### Amiga version:

- Any Amiga with an hard-disk and at least 2.5 MB of free RAM. Kickstart 2.0 or above recommended.
- Any 68K processor. For decent speed, recommended 68020 at 14MHz and FPU.

#### All versions:

- Some hard-disk space, enough to store the sample files and other temp data.
- Some old tapes that don't convert... :-)

---

## 3. Features list

- **Supports VOC, WAV, IFF, CSW and OUT formats.** No limitations over the file size. Only uncompressed, 8-bit mono sample files are accepted at the moment (exception: stereo and 16-bit WAV files allowed). For [OUT files](#9-how-to-use-out-files) and the amazing 100% failure-proof conversion, read the dedicated section in this manual.
- **Soundcard DirectMode.** **Realtime** conversion from the soundcard's input, **offering the same high reliability as offline operations!** It **decodes while sampling**, showing events in the exact moment when they happen. Read the next section for more info.
- **Special support for commercial loaders.** All the most common protection schemes are supported, including **all SpeedLocks (1-7), Alkatraz, SoftLock, BleepLoad**, **Paul Owens**, **Activision/Multiload**, **PowerLoad**, **Injectaload/Excelerator** and Ramsoft's exclusive **ZetaLoad.** All the loading details are shown, thanks to the internal code analyzer which desumes lots of low-level information about the data being processed, such as start addresses, block sizes, RAM pages, checksums and so on.
- **Loader type autodetection.** Since version 2.20, MakeTZX can automatically detect what kind of turbo loader you are converting and switches to the proper operating mode, so that you don't need to specify the loader type manually in the command line anymore. Remember to enable autodetect mode specifying the `-a` switch.
- **Advanced Error Correction.** Second-pass speculative analysis: it can correct many altered bits, even if the sample file is completely wrong in those points and reports the opposite logical value. Efficient pulse filter, many signal distortions are eliminated. Automatic frequency adjustments.
- **Powerful digital filter.** A highly optimized signal processing engine reduces noise and disturb very effectively. If available, takes full advantage of 3DNow! technology for considerable performance boost (faster than P2 at the same clock frequency). Implements a fully configurable DSP filter designer to allow the expert user to achieve excellent results. The filter can be applied both to ordinary sample files and to realtime DirectMode conversion.

---

## 4. Soundcard DirectMode (realtime)

MakeTZX is able to read samples directly from your soundcard without having to use a sampler program first to produce VOC or WAV files and so letting you save some extra time. Note that it doesn't simply record a sample file and then converts it, but it works in **true realtime**, that is it decodes while sampling and everything is showed on the screen in the exact moment when it happens!

The I/O engine uses DMA to guarantee the maximum performance and quality. Even on slow computers, MakeTZX won't miss a single sample and will not be affected by noise and nasty clicks which might interfere with decoding. So, realtime conversion offers exactly the same reliability as the standard vocfile mode and guarantees the same high conversion rates. Use it freely, it won't make you lose time! If you want, you can save the samples in a WAV file with option `-rk` and reuse them later if something goes wrong or you want to reprocess the file later. You can also activate the built-in digital filter to reduce noise and disturbs on the tape (see the filter section in this manual).

The Windows and Linux versions of MakeTZX work with any soundcard installed. The MSDOS version, instead, supports *SoundBlaster/Pro/16/AWE/32/64* and *Windows Sound System* (CS4231 / AD1848 families) compatible cards (including Ensoniq Soundscape). Most ISA-bus soundcards support at least one of these standards, so check the specifications of the board carefully. Usually, WSS soundcards are also SoundBlaster compatible and you may choose in which mode they operate under DOS. In this case, it is preferrable to configure the device for Windows Sound System mode because the SB emulation usually doesn't work correctly when sampling. Note that the Windows Sound System codec is detected and used before Soundblaster; if you want to skip WSS detection, you must specify switch "-rb" which will detect Soundblasters only.

#### Notes for the MSDOS version

MakeTZX requires an environment string reporting the I/O address, IRQ and DMA channel information.

- *For SoundBlaster cards*, ensure that the BLASTER string is defined. MakeTZX needs something like BLASTER=A220 I5 D1 (address=220H, IRQ=5 and DMA=1)
- *For Windows Sound System cards*, ensure that the WSSCFG string is defined. MakeTZX needs something like WSSCFG=A530 I7 D1 (address=530H, IRQ=7 and DMA=1). If necessary, use the configuration utility provided with the soundcard to adjust the volume settings and select the proper input source (e.g. LINE). If you can't do it, use switch `-i` to do it manually.
- *For Ensoniq Soundscape cards*, ensure that the SNDSCAPE string has been defined by your drivers. MakeTZX will retrieve all the necessary I/O information by itself. In general, refer to the WSS considerations too.

MakeTZX will not use a soundcard if its environment string is not defined.

#### Notes for the Linux version

MakeTZX uses the Open Sound System standard (OSS) and accesses the soundcard through the */dev/dsp* device. Please make sure that sound support is enabled and working in your Linux box, either compiled into the kernel or loaded at startup as a kernel module. You must use your favourite mixer program to adjust the input level settings and to select the sound source for recording (e.g. LINE-IN, MakeTZX's default); if you play the tape but the vu-meters remain stuck at zero, you need to check the mixer settings. The pause key is not yet implemented. Follow the DOS DirectMode instructions.

#### Notes for the Windows version

DirectMode under Windows is straight-forward. Since MakeTZX uses the standard Windows sound drivers, there are no compatibility issues whatsoever. Any soundcard with installed drivers will work just fine. By default, MakeTZX will use the default device for sampling (as defined in the Multimedia applet of the Control Panel), but if you have more than one soundcard installed you can specify any other device with the switch "-rd". For instance, if you want to use device #2 then add "-rd2" to the command line. If you specify an invalid device number, MakeTZX will print the full list of devices available in your system and then exit.

#### Enabling DirectMode

To start MakeTZX in DirectMode, you need to specify the -r switch optionally followed by the shifted switch 's' and the desired sample rate (the default value is 32258 Hz). See the [command line chapter](#5-command-line-parameters) for all the possible switches regarding DirectMode. Due to MakeTZX internal engine requirements (basically for Advanced Error Correction), the samples are written to disk anyway and so the maximum recording time is limited by the available disk space on the local unit. If you are getting out of space, try reducing the sample rate to produce a smaller tempfile. To set the maximum recording time you should use the shifted switch 't' followed by the number of seconds or minutes and seconds in the form 'm:s'.

#### Setting up the tape player

Before the conversion starts, **two level meters** appear at the top of the screen to allow volume adjustments and other tunings. The first bar (green) is a vu-meter which shows the signal amplitude; the second bar (red) measures the amount of sample clipping, that is how much the signal is distorted by excessive volume. The best condition is when you get the green bar as big as possible while keeping the red bar as small as possible. **To find the optimal level,** follow this procedure: starting from zero, increase the volume of your tape recorder gradually and you will see the green bar growing accordingly while the red bar remains stuck at zero. At a certain point the red bar starts to grow too and this means that you reached the optimal level. You may safely keep the red bar at around 10% or 20% without affecting the signal quality significantly.

When you are happy with level adjustments, **press any key to start the conversion** and watch the loading info appearing on the screen in perfect timing with the tape signal!

#### Control keys

**To pause the recording,** press 'p' at any time followed by any other key to resume. During pause, the vu-meters are shown again. The recording can be stopped in manual or automatic mode. **To halt the conversion manually**, simply press any key (but 'p') at any time. **For automatic mode,** instead, set a **preprogrammed recording** specifying the exact duration of the process with shifted switch 't' as said above; for example, to record for 5 minutes and 30 seconds enter "-rt5:30" and after that amount of time has elapsed MakeTZX will automatically stop. Of course you can still halt operation manually at any time also during a preprogrammed recording.

#### MSDOS DirectMode TROUBLESHOOTING (compatibility notes - MSDOS version only)

**Note:** Besides the following paragraph, don't forget to read also the **[Frequently Asked Questions](#13-faq-frequently-asked-questions)** for further troubleshooting about DirectMode.

The SoundBlaster driver has been developed and tested on original Creative SBPro2, SB16 and SB32 (DSP 4.13). It requires 100% SB compatible ISA chipsets because most clones don't implement the DSP sampling commands in the correct way (if not at all), while playing usually works. All modern soundcards use the PCI bus, and they rely on software drivers for compatibility with the old classic models. In general, you may encounter two kind of problems with MakeTZX: garbage samples (the vu-meters go mid-scale even without signal) and - oops - system hang. The first case has been reported with PCI cards (SB PCI64/128, SB Live) and the Vibra VX SB16 chipset (which uses the same DMA channel both for 8 bit and 16 bit samples) ; we haven't got any of these cards so it is a bit difficult to guess how to fix the code. The second case can be solved with [compatibility mode](#soundblaster-compatibility-mode) sometimes (see below).

For Soundblaster cards, the realtime engine requires to access the soundcard in autoinit DMA mode, which is officially available only on SB2 or above. Soundcards reporting DSP versions 2.00 or 2.01 might actually be old v1.5 (especially for compatibles) and so autoinit DMA could not be available or work improperly. We allow these DSP's anyway because some true SB2 might work as well. Having a DSP greater or equal than v2.02 should be always ok.

Please note that DSP v3.xx or above can sample up to 45454 Hz, DSP 2.02+ up to 22727 Hz and DSP 2.00 and 2.01 up to 22222 Hz. MakeTZX automatically checks for the valid ranges. However, fake SB2's (actually SB1.5's) might be able to sample only up to 16 KHz, but we cannot check this from our code because we cannot distinguish true SB2's from the others. Besides, these rules apply to Creative models for sure, but we are not sure about ALL compatibles. For example, I seem to recall that my friend's Mediavision Thunderboard reports DSP v2.01 but can sample up to 16 KHz only. Other models cannot sample over 12 KHz.

Another issue is about the soundcard mixer. We let you do all the settings before sampling, such as input level, source selection, bass/treble and so on. Make sure you enable output if you want to hear the signal from the speakers while sampling. There are many (uncompatible) mixer chips out there, especially on SB clones; some of them require speaker disabled while sampling, others the opposite, so it's not easy to guess the initialization... please report any problems!

If you are experiencing problems that cannot be solved reading carefully this manual, it's important that you take note of everything MakeTZX prints and send it to us with a detailed description of your system: exact soundcard model, IRQ and DMA channels, operating system, command line used to start MakeTZX and so on.

#### Soundblaster Compatibility Mode

Some Soundblaster clones require to be accessed in the same way both for low and high sampling rates, while original SB's need two different command sets. If your soundcard hangs or MakeTZX reports an "IRQ LOCK" error, try with option "-rc". See the FAQs too.

#### Soundcard IRQ lock

When DirectMode is enabled, MakeTZX performs a preliminar I/O check to ensure that the soundcard communicates with the DMA controller correctly. If there's something wrong, after two seconds MakeTZX exits automatically and reports a message like "FATAL ERROR: IRQ #N LOCKED", which means that a timeout has occurred because the soundcard didn't send any data.

Possible reasons for this event are:

- *PCI soundcard:* modern PCI cards (e.g. SB Live, SB PCI128, etc) support the legacy SoundBlaster models exclusively via software emulation. Typically, software emulation works pretty good for playback (e.g. with DOS games and emulators), but we've never seen any software drivers able to work in sampling mode. So, if you have a PCI soundcard and you get this error, you should try the WinGUI DirectMode instead.
- *Wrong IRQ number:* The BLASTER (or WSSCFG) environment string reports an IRQ number that does not correspond to the real one. Use a diagnostic program to determine the IRQ line of your soundcard and update the BLASTER (or WSSCFG) string accordingly.
- *Soundcard DMA error:* If you are working in SoundBlaster mode, try with option "-rc" which enables Soundblaster compatibility mode and tells the soundcard to access autoinit DMA in a different way. As we said earlier, MakeTZX requires a special DMA mode and your soundcard may not be able to handle it. If MakeTZX gets to this point, then you surely have a DSP greater or equal than v2.00 (which should be capable of autoinit DMA) and so it might be a compatibility issue. If compatibility mode doesn't work, check if other SB sampling programs work with your soundcard (not Windows programs, of course) and, in this case, please send us a detailed description of your card and the system configuration that you use. The switch "-rc" has no effect with Windows Sound System mode.

---

## 5. Command line parameters

This section explains the various options of MakeTZX. It is essential that you read it carefully in order to understand how MakeTZX works and what it can do, so that you can make full use of all its powerful features.

The syntax couldn't be simpler:

**MAKETZX samplefile[.EXT] [tzxfile[.TZX]] [switches]** (see examples later)

where EXT is one of the supported extension types and can be omitted (items in square brackets are optional).

Actually, switches and filenames can be specified in any order you prefer. If you don't specify the output file, a TZX with the same name of input file (but extension .TZX) will be created.

The program will attempt to open a file named 'samplefile'. In case it will fail and the extension wasn't specified, it will then attempt to open the same file with VOC, WAV, CSW, IFF and OUT extensions in that order. If you specify two filenames when using DirectMode (realtime), the first filename will be ignored.

You don't need anymore to specify the loading scheme in the command line if it is Alkatraz, Speedlock 1/2/7, Bleepload, Paul Owens, Softlock, PowerLoad, Multiload, Injectaload, Excelerator or Zetaload, since now MakeTZX can automatically detect them. All you have to do is enable the autodetect mode with "-a" (see the table).

If you want to override autodetect, you can specify a loading standard manually by using a particular "-l" switch; if none is specified then the program will operate in RAW mode. RAW mode can decode any turbo loading system including Alkatraz and Softlock (it will automatically try to detect the loading speed) but not Speedlock and Bleepload. If you specify both "-a" and "-l", the manual setting will override autodetect mode.

Here's the complete list of all available switches (also displayed with "*MAKETZX -?*"):

| Switch | Description |
|---|---|
| `-a` | Enable **loader type autodetect**. You don't need to enter any -l switch, MakeTZX will automatically identify the loading scheme for you. At the moment, the following schemes are recognised: Alkatraz, Speedlock 1/2/7, Bleepload, MultiLoad, Paul Owens, Softlock, PowerLoad, Zetaload, Injectaload, Excelerator and Biturbo. If autodetect should fail to identify the correct type, please retry using the manual setting (switch "-l"). Autodetect mode will work in RAW mode until a specific loader is found. |
| `-ln` | Tells the program to operate in **NORMAL** mode only, i.e. with the timings of the **ROM** loader. Recommended when all the blocks are standard ones. |
| `-lnf` | Enable "fast pilots" detection for normal blocks with short pilots (e.g. LERM). |
| `-la` | This is used when converting programs protected by the **Alkatraz** protection system (games like Ghouls'n'ghosts, Ghostbusters II, 10th frame, Forgotten worlds and many others). |
| `-lf` | Enables the **Softlock** decoding algorithm (games like Cylu and Chimera). |
| `-lb` | Use **BleepLoad** scheme (used on most Firebird games like Bubble Bobble). |
| `-ls<N>` | Use [**SpeedLock**](#7-how-to-identify-a-custom-loader) type N scheme, with N ranging from 1 to 7. Click on the link above to learn how to identify the different SpeedLock variants. |
| `-lz` | Use the Ramsoft **ZetaLoad** scheme (if you have any program from us). |
| `-lo` | Use the **Paul Owens** protection system (games like Batman the Movie, Operation Thunderbolt, The Untouchables, Chase HQ, Rainbow Islands, Red Heat) |
| `-lm` | Use the **MultiLoad** protection system (former Activision, games like Pacland, Timescanner, Ninja Spirit, Dynamite Dux) |
| `-lp` | Use the **PowerLoad** protection system (games like Arena, Deathwake, Dynamite Dan) |
| `-li` / `-lie` | Use the **Injectaload** (`-li`) or **Excelerator** (`-lie`) protection system (games like Loads of Midnight, Book of the Dead, Jack the Ripper) |
| `-lg[N]` | Use the **Biturbo** scheme by GB Max, found on all the Special Program and Special Playgames italian tape magazines. You may optionally specify the loader subtype N (1-3) or let MakeTZX autodetect it for you leaving `-g` only. |
| `-k<N>` | Tells the program to override the default number of blocks to **skip** before considering encoded data blocks. For example, if you have a BASIC program followed by a Speedlock 5 game, use "-k2 -ls5" |
| `-x` | Displays file informations in **hex** form (except for block numbers and pauses). |
| `-h` | Disables **hi-fi** decoding tones reproduction (for Speedlock loaders). By default, MakeTZX generates Speedlock tones with an algorithm which emulates faithfully the bubbling sounds. This options turns off the algorithm. |
| `-r` | Enable [**soundcard DirectMode**](#4-soundcard-directmode-realtime) (default rate: 32258 Hz). Realtime conversion from the soundcard (on supported platforms). Click on the link above for all the details. |
| `-rs<N>` | Set **sampling rate** to N Hz (min. 5000 Hz, max. 45454 or 48000 Hz). Check that your soundcard can record at the desired frequency. Under MSDOS, only WSS soundcards can sample up to 48 KHz. Most modern PCI cards can do 48KHz. Under Windows/Linux, we recommend using "fair" sampling rates (e.g. 22050, 32000, 44100 or 48000). |
| `-rt<s>` / `-rt<m:s>` | Set **preprogrammed recording time**, e.g. -t 5:30 (no spaces) or directly in seconds: -t 330. |
| `-rk` | Save the sample generated by DirectMode in **WAV** format. Use this if you think you may need to reprocess the file later (so you don't have to take another sample of the tape). Generally recommended for safety. |
| `-rc` | Enable SoundBlaster [compatibility mode](#soundblaster-compatibility-mode). You may try this if standard DirectMode doesn't work with your soundcard (see also other [DirectMode](#4-soundcard-directmode-realtime) compatibility issues). Has no effect on WSS cards. (**MSDOS** only) |
| `-rb` | Skip Windows Sound System cards detection. This forces detection of SoundBlaster cards only. (**MSDOS** only) |
| `-rf` | Enable **digital filter**. This switch is obsoleted by the new `-f` group. Please read the [related manual section](#11-the-digital-filter) to learn how to get the most from it. |
| `-ri<N>` | Select **input source line** for Windows Sound System soundcards. Use this switch when you can't do it from the soundcard's mixer program. The number N can be: 0= LINE-IN, 1= AUX-IN, 2= MIC. If N is omitted (leaving `-i` only), then 0 (line-in) is assumed. This setting is ignored for SoundBlaster cards. (**MSDOS** only) |
| `-rd<N>` | Select the **sampling device** under Windows. Use this switch if you have more than one soundcard in your system and you want to use one in particular. The number N ranges from 0 to the number of devices available minus 1. To get the full list of devices for your system, specify an invalid number (e.g. -rd100). (**Windows** only) |
| `-f` | Enable [**digital filter**](#11-the-digital-filter), a powerful and effective signal processing tool which efficiently eliminates noise and disturbs. Please read the [manual section](#11-the-digital-filter) to learn how to use it and for some important information. Best used in DirectMode. |
| `-ft<N>` | Set **filter type** where N is: 3= Low pass, 4= Band pass (default), 5= High pass. |
| `-fl<N>` | Set the **lower cut-off frequency** for band-pass and high-pass filter types. |
| `-fh<N>` | Set the **upper cut-off frequency** for band-pass and low-pass filter types. |
| `-fo<N>` | Set **filter order**. Specifies how strong is the filter. The default value is 4. Using excessive values may be counterproductive. |
| `-fp<N>` | Set **filter prototype** where N can be: 1= Butterworth (default), 2= Chebyshev |
| `-fr<N>` | Set **ripple** for Chebyshev filters. The default value is 1.00. This setting is ignored for the Butterworth model. |
| `-f3` | Disable **3DNow!** optimizations. This prevents the use of the optimized code also when a compatible processor is found. (**x86** platforms only) |
| `-b` | Enable TZX **beautifier**. This function improves the overall quality of the TZX file by adjusting the pauses after the various data blocks to round values, for better looking (and playing) tapes. Apart from the cosmetic purpose, it is also useful to make such intervals more regular and homogeneus (e.g. between game levels). |

Now, note that the list above basically condenses into three switches only: the loader type (group -l), the realtime options (group -r) and the filter options (group -f). Besides, the autodetect feature eliminates the need for -l switches in most cases, so only two groups remain and they are not essential for conversion!

To make it clear, we give some examples of commands for the most common cases. Sometimes, two or more alternatives are given to do the same job.

### COMMAND LINE EXAMPLES

| What you want to do | How to do it |
|---|---|
| Convert a generic turbo or normal speed program recorded in "game0.voc" | `> maketzx game0`<br>(*will produce game0.tzx*) |
| Convert a Speedlock 7 game recorded in "game1.voc" | `> maketzx -a game1` (*using autodetect*) **or**<br>`> maketzx -ls7 game1`<br>(*will produce game1.tzx*) |
| Convert a generic turbo game in realtime (DirectMode) with the default settings | `> maketzx -r game2`<br>(*will produce game2.tzx*) |
| Convert a game in realtime (DirectMode) using autodetect mode and the digital filter with the default settings | `> maketzx -a -r -f game2`<br>(*will produce game2.tzx*) |
| Convert an Alkatraz game in realtime sampling at 44.1 KHz and save the samples | `> maketzx -a -rs44100 -rk game3` (*autodet.*) **or**<br>`> maketzx -la game3 -rs44100 -rk`<br>(*will produce game3.tzx and game3.wav*) |
| Convert a program at normal speed using DirectMode with autostop after 7 minutes and 30 seconds, on a soundblaster clone | `> maketzx -rt7:30 -rc -ln game4`<br>`> maketzx -rt450 -rc -ln game4` (*less evident*)<br>(*7 min. and 30 sec. = 450 seconds*) |
| Convert a game of an autodetected loader type in "game5.wav", processing it through a 1st order lowpass filter with cut-off frequency of 8 KHz | `> maketzx game5 -a -fo1 -ft3 -fh8000` |
| Convert a Softlock game which comes after a BASIC program (file "game6.voc") | `> maketzx game6 -k2 -lf` |

If you have problems entering MSDOS commands, use one of the graphic interfaces available for MakeTZX (both for MSDOS and for Windows)!

**NOTE:** you can specify only **one** decoding algorithm (otherwise an error message is given). Manual loader type specification disables autodetect mode (if enabled).

**WARNING:** to use more than one shifted switch you have to **retype** the main switch (example: -rs22050 -rt15:20)

---

## 6. Understanding MakeTZX's messages

While decoding, MakeTZX produces several messages on the screen to inform you about what's going on. They are very important because everything about the conversion process is reported there: data blocks found, start addresses, ram pages, decoding status, warnings, error messages and so on.

In SoundBlaster DirectMode all the messages are shown on the screen in realtime, so you will see them in the very exact moment when the events happen! It's like on the real Spectrum! We recommend you to try it (for example with a Speedlock, it's amazing), fun is guaranteed!

Basically, you will presented with two kinds of information:

### System status

Usually, the last printed line of the screen shows what MakeTZX is doing in that moment. You will read any of the following:

- **Searching pilot:** MakeTZX is searching for the leader tone of a data block.
- **Loading pilot:** The decoder is tuned with the signal and it is waiting for a sync pulse to start reading the data bits. If an error occurs here (strong clicks and noise, or a bad sync pulse), the leader synchrnization is lost and MakeTZX goes back to the previous phase.
- **Loading clicking pilot:** MakeTZX is loading the clicky pilot tone of a Speedlock 1-3 program. If the clicking pattern does not match with the Speedlock standard, loading is aborted.
- **Reading data:** The synch pulse has been successfully found and now data is being read bit by bit at the estimated speed. Several errors may occurr in this phase which terminate the data loading.
- **Recovering data:** something went wrong in the first decoding attempt so MakeTZX is trying to recover the correct data stored on the audio stream.
- **Finding pause:** The bit stream has stopped (end of the block or loading error) and MakeTZX is now estimating the time before the next block. In this phase it is usually waiting to detect a pilot tone. Errors occurring now have no consequences.

### Data block information

For each decoded block, MakeTZX prints everything it can desume about it. The shown info depends on the operation mode (e.g. Normal, Alkatraz, Speedlock, etc.), since for each custom loader MakeTZX knows the programming details of the machine code.

In each line, the block length and the pause (i.e. the time between the end of that block and the beginning of the following one) will be always shown.

- **For standard blocks (any speed):** if the block is a header, MakeTZX prints filename, start address, length and file type (also for the following data block). This applies also for the standard blocks of custom turbo loaders (such as the BASIC loader). If the block has non-standard timings, the relative speed respect to the ROM defaults is shown.
- **Speedlock and Zetaload:** Start address, length and RAM page for each block and sub-block, together with the respective IDs. MakeTZX also computes all the checksums to validate the correct decoding.
- **Alkatraz:** Start address, lenght and checksum for the first turbo block. Any information (but length) for the successive blocks is almost impossible to determine.

**Important:** after the filename (if available), MakeTZX prints a status character, with the following meaning:

| Symbol | Meaning |
|---|---|
| `-` | The block has been successfully decoded without any problems. |
| `?` | The data integrity cannot be assured because the checksum is not available. |
| `R` | Data corrupted but successfully restored by Advanced Error Correction. |
| `!` | Data corrupted under exponential complexity, but it could work anyway. |
| `F` | Data corrupted and AEC failed recovery (unsupported fault class). |
| `X` | Bad checksum, but no errors detected. |

---

## 7. How to identify a custom loader

This is a brief guide to help you recognize the various custom loaders explicitly supported by MakeTZX. Remember that most of the following schemes are autodetected, so you only need to turn on autodetect mode (switch -a) and MakeTZX will identify the correct loader type for you. See the command line section for a list of the autodetected types.

### Alkatraz

A very fast and popular scheme, used in all US Gold games. It's very easy to recognize:

- Single BASIC loader block. The header doesn't print the usual "Program:" prompt.
- A quite long turbo block with the usual blue/yellow stripes, followed by a pause of about 10/15 seconds. The loading sounds appear completely random without any repeated structure, like white noise, due to the heavy encryption pattern.
- The screen starts to load in a very fancy way, different from game to game; for example, you could see it appearing line by line from top to bottom or vice versa, or some regions could be loaded before others, etc. The block is preceded by a very short pilot tone, so short that it is hard to hear it clearly.
- After the screen has loaded, a three-digit countdown appears. The border is black without any stripes. The main block can be quite long, especially with 128K programs. It is very fast, nearly 151% of normal speed.
- In case of error, a b/w flashing message appears at the bottom of the screen and an annoying beep is played for some seconds, after which the computer is reset. The message says "Tape loading error: rewind & reload" or something like that.
- Level data are recorded in the same format, with yellow/blue stripes.

Some examples of Alkatraz games are LED Storm, Ghouls'n'Ghosts, Indiana Jones and the Last Crusade, Street Fighter, HKM, The Goonies, and many many more.

### Speedlock

Speedlock is one of the most common commercial turbo loaders. It can be found in hundreds of games, many of which from Ocean. Although there are several versions around, you can recognize it because in all variants the loading screen appears suddenly, there are no coloured stripes in the border when the screen has appeared and there is often a countdown timer showing the remaining time expressed minutes, seconds and tenths of a second.

There are seven different versions of Speedlock loaders, plus a hybrid one (see the notes).

Speedlock types 1, 2, 7 and the hybrid 1/2 can be autodetected (switch "-a"), so you may want to let MakeTZX try to recognize them for you.

Please read the notes.

#### Type 1

- 1 or 2 normal speed blocks (both BASIC, first one very short)
- clicking pilot
- STANDARD colours
- high speed loading (nearly 150% of normal)
- no encryption
- 48k only programs

Examples: Highway encounter, JetSet Willy, Daley Thompson's Decathlon Day 1 and 2.

#### Type 2

- 2 normal speed blocks (both BASIC, first one very short)
- clicking pilot
- NON standard colours (RED/BLACK - BLUE/BLACK)
- high speed loading (nearly 150% of normal)
- ENCRYPTED data
- 48 and 128k programs

Examples: Enduro racer, Knight rider.

#### Type 3

- 2 normal speed blocks (one BASIC, one CODE)
- Bubbling sounds (NOT needed by decrypter) and flashing colours on border while decrypting.
- clicking pilot
- NON standard colours (RED/BLACK - BLUE/BLACK)
- high speed loading (nearly 150% of normal)
- ENCRYPTED data
- 48 and 128k programs

Examples: Leviathan 48 and 128k

#### Type 4

- 2 normal speed blocks (one short BASIC, one long CODE)
- Bubbling sounds (NOT needed by decrypter - i.e. you could stop the tape) and flashing colours on border while decrypting.
- NON clicking pilot
- NON standard colours (RED/BLACK - BLUE/BLACK)
- high speed loading (nearly 150% of normal)
- ENCRYPTED data
- 48 and 128k programs

Examples: Athena 48 and 128k, Catch 23

#### Type 5

- 2 normal speed blocks (one short BASIC, one long CODE)
- Clicking tone or wave sequence (NEEDED by decrypter - i.e. if you stop the tape the computer will reset) and flashing colours on border while decrypting.
- NON clicking pilot
- NON standard colours (RED/BLACK - BLUE/BLACK)
- high speed loading (nearly 150% of normal)
- ENCRYPTED data
- 48 and 128k programs

Examples: Out run, Ping Pong, Winter games 1 and 2, Madballs.

#### Type 6

- 2 normal speed blocks (one short BASIC, one long CODE)
- Clicking tone or wave sequence (NEEDED by decrypter - i.e. if you stop the tape the computer will reset) and flashing colours on border while decrypting.
- NON clicking pilot
- NON standard colours (RED/BLACK - BLUE/BLACK)
- MEDIUM speed loading (nearly 127% of normal)
- ENCRYPTED data
- 48 and 128k programs

Examples: Vixen, Super hang-on.

#### Type 7

- 1 normal speed block (long BASIC)
- NO ACTION OR SOUNDS while decrypting.
- NON clicking pilot
- NON standard colours (RED/BLACK - BLUE/BLACK)
- MEDIUM speed loading (nearly 127% of normal)
- ENCRYPTED data
- 48 and 128k programs

Examples: Turrican, Myth, Operation Wolf 48 and 128k, Firefly, Arkanoid II, Tusker, The last ninja II, and many others!!!

**NOTE:** type 2 is very similar to type 1, type 4 is similar to type 3 and type 6 is similar to type 5. If you don't succeed in correct conversions please try even with more than one type.

**NOTE:** The hybrid Speedlock 1/2 (e.g. Highlander) must be decoded as type 2.

### Paul Owens

It was used in all the latest games from Ocean.

- Single BASIC loader, always the same from game to game (with different names, of course)
- The blocks show red/black stripes both for the pilot tone and the data.
- The screen appears suddenly and it is preceded by a small block of data. The main program is loaded as a separate block from the screen.
- It is not a very fast loader, just a little more than the standard ROM speed (about 107%). The noise produced is a bit particular.

Examples of Paul Owens games are Batman the Movie, Rainbow Islands, Chase HQ, Red Heat, Cabal, The Untouchables and others.

### Bleepload

It was used in many Firebird games.

- Two normal blocks (BASIC and CODE).
- It prints the name of the game before the loading screen.
- Yellow/red stripes for the pilot tone and cyan/blue for data.
- Each data unit is split into many small blocks recorded at normal speed.
- A hexadecimal upward counter is shown on the screen.

Examples of Bleepload games are Bubble Bobble, Starglider and others.

### Activision/Multiload

As the name suggests, this loader was used for some Activision games.

- There are no stripes on the border while loading.
- The program is loaded as a single big block recorded at normal speed.
- The loading noise is quite periodic, due to the regular encoding/encryption system which produces fixed bits at regular intervals.
- Old versions of the loader show some animated instructions and a counter during loading.

Examples of Activision games are Pacland, Timescanner, Ninja Spirit, Dynamite Dux and others.

### Softlock

It can be a normal or turbo speed loader, used in a few games

- Two standard blocks (BASIC + CODE)
- Either green/magenta stripes for the pilot tone and black/white for data, or the normal red/cyan and yellow/blu colors.

Examples of Softlock games are Impossible Mission 2, Cylu, Rasputin, Elite.

### PowerLoad

It is a quite fast turbo loader, used in a few games

- Two standard blocks (BASIC + headerless CODE)
- Standard colors.
- Usually the screen attributes start to appear from the bottom to the top.
- Two variants: turbo and normal speed.

Examples of PowerLoad games are Arena, Dynamite Dan, Deathwake. Examples of Injectaload games are

### Injectaload and Excelerator

These two loaders are almost the same one, and they were used by CRL.

- Two standard blocks (BASIC + CODE)
- One normal speed block of 1000 bytes containing the turbo loader (many decrypters); red/black stripes in Excelerator, normal colors in Injectaload
- Muilticoloured stripes in Injectaload, normal colors in very thin stripes over a black background for Excelerator.
- Two variants: one single block for 48K games, more blocks for 128K ones.
- Data are saved with 9 bits per byte. Simple but effective encryption system.

Examples of Injectaload games are Ninja Hamster, Book of the Dead, 3D Game Maker. Some Excelerator titles are Jack the Ripper, Loads of Midnight, The Last Mohican.

### Biturbo

This scheme has been used in all the italian tape magazines of the series Special Program, Special Playgames, Program and Playgames; they were very popular in Italy in the 80's and early 90's. There are three subtypes:

#### Biturbo 1

- Short BASIC loader.
- The program is loaded in a single big block, screen included.
- Rainbow stripes (all 8 colors ordered).
- It's the fastest turbo we have ever encountered (3600 baud peak).

#### Biturbo 2

- Very similar to type 1, but the screen is loaded as a separate block before the program.
- Coloured stripes will all 8 colours, but unlike type 1 these are not ordered in a rainbow.
- Still very fast.

#### Biturbo 2.5 and 3

- These are totally different from types 1 and 2. After the BASIC block, a coloured pattern is shown on the screen (similar to the test mode of 128K Spectrums). At the bottom, it prints "Biturbo II" (type 2.5) or "Biturbo III" (type 3) and the Spectrum model (48 or 128K).
- A counter is shown both while loading the screen and the main program.
- Type 2.5 has stripes similar to type 2, while type 3 has thin coloured stripes in a black background.
- Much slower than types 1 and 2.

Biturbo 1 appeared in the earlies issues of the magazines, while Biturbo 3 was used in the latest. Many issues were recorded with type 2; only a few issues have type 2.5.

### Zetaload

Zetaload is a very fast custom turbo loader created by us (Ramsoft). We have used it for some special releases in the past.

- Short BASIC loader followed by a short turbo block with blue/black stripes.
- Appears similar to a Speedlock 7, but it is much faster and better encrypted.
- The screen appears suddenly and there are no stripes on the border (most times).
- While loading the main program, a progress bar is shown on the screen.

---

## 8. The CSW file format

CSW files are a way of storing sample data in a compact form, typically taking 1/10th of an ordinary VOC. It is used internally by MakeTZX, but it is also very useful to keep down the disk space taken by your VOC/WAV files. The CSW utility can handle CSW conversion in both ways (see below); you can download it separately at our web site. Of course, MakeTZX itself accepts CSW files for input.

When converting to the CSW format, the sample file is processed through MakeTZX's internal digital filter which reduces noise and signal distortions very efficiently. Make a backup copy of the original file if you will need the original samples later, but remeber that in most cases the CSW will be a lot better than the original file.

Note that CSWs are intended for use with **square waves only** (such as computer tapes)!

The compression ratio depends on many factors; in general, the higher the sample rate, the higher the ratio. A clean and regular signal helps too. The ratio for a 44 KHz file will usually be twice the value for a 22 KHz one. The typical gain for a 44 KHz turbo tape is about 93%, which means a 12:1 compression factor! Normal speed tapes should compress even better.

Finally, CSW files are highly compressable with the standard PC archivers such as RAR and ZIP. The packed CSW files are usually smaller than the zipped original VOCs. You will be able to RAR a 40 MB sample file down to a few hundreds KB.

**[Download CSW utility](http://www.ramsoft.bbk.org/maketzx.html)** (with fully-detailed manual)

---

## 9. How to use OUT files

OUT files are trace files produced by emulators such as Z80. They are used by MakeTZX to achieve the ideal integration between an emulator and a TZX decoder, a revolutionary approach which allows MakeTZX to reach the mathematical 100% successful conversion rate! OUT files are processed by MakeTZX exactly like the other sample files (VOC, WAV, etc.), so the same rules and considerations apply but with the big difference that the conversion just can't fail! Due to its intrinsic nature, this technique can be applied to the vast majority of custom loaders around, but with a very few types it doesn't work yet. This topic is discussed later in this chapter.

Anyway, in all the supported cases, **any program which loads and runs into the emulator will be perfectly converted by MakeTZX without any error**. This is an amazing result, which definitely puts an end to the TZX conversion problem in most situations.

### How to produce OUT files.

We refer to the Z80 emulator because at the time of writing it's the only one supporting the OUT format. You can possibily do the same with other emulators.

1. Start the emulator and tell it to log ANY out instructions to port 0xFE (254 decimal). In Z80 this can be done by launching the emulator with switch `-xg` (very important) and then pressing F10, X and O in sequence; then press N and enter the filename for the OUT file that will be produced. OUT logging is now automatically enabled in Z80.
2. Make sure that port 0xFE (254 decimal) is included into the list of the I/O ports logged by the emulator; in Z80 it's included by default. If not, add it by pressing A and then typing FE. You can also log other ports such as AY-38912 (128K music); they will not interfere with MakeTZX.
3. Select the VOC file to play and then load the program to convert normally (i.e. with LOAD "" or using the Tape Loader of 128K Spectrums). Of course you can also use real tape support (from LPT or Soundblaster) if you prefer.

When loading is complete, exit the emulator. You can now process the OUT file produced with MakeTZX as usual like a common sample file.

### Applicability

The primary purpose of this technique is to solve those cases when a VOC file works in the emulator but MakeTZX fails to convert it successfully in other ways and you have already tried all the possible remedies such as tweaking with the digital filter. However, this approach has proved to work so well that you may want to use it often because it guarantees 100% success without any troubles.

We will now discuss the main points of this technique, showing advantages and disadvantages.

- If a program works in the emulator, it guarantees **100% successful conversion** at first attempt without any trouble, regardless of the conditions of the tape and all the other factor which normally disturb the decoding process (such as noise, spikes, etc.). This is the best result possible that you may ask to a TZX converter!
- It is known to work with the following loaders: ROM (normal loading), Alkatraz, Speedlock 5 to 7, Softlock, Bleepload, Paul Owens, GB Max Biturbo, Zetaload. Besides, it works also with Sinclair User and Your Sinclair turbo loaders, as well as many others.
- It is known not to work with the following loaders: Speedlock 2 to 4; untested Speedlock 1 and Activision.
- Other loaders are very likely to work. The vast majority of tape loading routines behave similarly to the supported types, so they will work with the technique. This means that nearly all the loaders you may encounter which are not explicitly listed here should work fine, in particular those which make coloured stripes on the border. Only very few of them are so particular to fail.
- Only the files actually loaded into the emulator will appear into the OUT file, and in that exact order. This means that to convert all the levels of a long multiload game like Turbo Outrun you must play the game until all the necessary files get loaded. Note that in 128K mode many games load all levels in one time, so they are converted perfectly with this technique (e.g. Chase HQ, Batman the Movie, Rainbow Islands and many more). Besides, in some cases you can use cheats to skip through the levels or tweak the game to load a given file even if it shouldn't.
- OUT files are a faithful recording of what happend during the loading process. In some circumstances, the pauses between blocks could be slightly different from real ones so you may want to adjust them (or enable the TZX beautifier function). Reasons are 1) the time variations caused by user interaction (e.g. delays while waiting for a keypress before loading a level, etc) and 2) the intrinsic nature of OUT files. Besides, you may need to check the decryption tones of some Speedlock programs if necessary.

**VERY IMPORTANT:** Please note that, while *every* VOC (or whatever else format) that runs in the emulator will be surely converted by MakeTZX, the converse is generally not true, that is MakeTZX will successfully convert many VOC files that won't load into the emulator! In other words, MakeTZX is a much better loader than the emulator and it is necessary to use OUT files only in the rare cases when MakeTZX fails to decode a tape which instead works in the emulator.

---

## 10. How to convert

As we said, MakeTZX was designed to do its job without requiring the user to set tons of parameters, so you will not have to retry a conversion many times, each time trying to guess the right combination of settings. The engine is intelligent enough to calculate the appropriate values for you, or at least it tries to. We have found that in 99% of cases (actually, 56 out of 57 in our test set) MakeTZX is successful, a percentage that we have not been able to achieve with any other similar tools, even tweaking with the settings.

However, sometimes it may happen that you are not able to produce a working TZX, especially if you have not a great experience with TZX decoding and you are at your first attempts. Before we go on with some useful suggestions to help you, we wish to emphasize some special features of MakeTZX that it is worth paying attention to:

- The digital filter. This is an extremely powerful tool with excellent capabilities; it is able to recover the most difficult tapes surprisingly well. We had excellent results using it, much much better than expected. It could be necessary to alter the default settings in order to achieve the desired effect, so you may need to read the [filter section](#11-the-digital-filter) to learn how it works. Believe us, the excellent results are well worth the time spent reading the filter description!
- OUT files. They allow 100% failure proof conversions when the sample file works with the emulator. Read the manual section dedicated to [OUT files](#9-how-to-use-out-files).

**Sampling**

Sampling is probably the most delicate phase of the whole process, so it is worth spending most of the time and the attention. A good quality sample is essential.

First of all, squared or non-squared doesn't make much difference for MakeTZX, since it has an internal digital filter which squares the signal and reduces noise and spikes. Do not confuse this "filter" with the digital filter (switch group "-f"), it has nothing to do with it. Use READSB if you are used to it, but it is not strictly necessary. Instead, **we recommend you to use DirectMode as much as possible**: it's reliable and will let you save a lot of time. If you think that the tape is in critical conditions or you don't know exactly what kind of loader type you are in front of, use switch "-rk" to save the sample file so that you can experiment with it later without having to resample the tape again!

If a conversion in DirectMode has failed, then it would have failed even if you had recorded the samples into a VOC with your favourite sampler. Besides, consider that a digitally filtered VOC is not always better than a normal one, because the digital filter itself may introduce nasty errors such as a 1-bit which becomes two 0-bits -- and try to get rid of this (although MakeTZX can)!

For a good signal quality, we recommend the following:

- Adjust the azimuth of the cassette player.
- Set the volume to a reasonable level, that is just below the point it starts to clip (read section 4 about the level meters). Note that a small percentage of sample clipping (say 10%) is generally desirable, since it helps to reduce some noise disturbs. If the volume is too low, MakeTZX could fail to detect some pulse edges, while if it is too high (resulting in a lot of clipping) the noise gets amplificated too and the Signal-to-Noise Ratio dies.
- The volume level should be determined in the middle of loading (i.e. during data bits) and not, for example, while the leader is playing, since these two kinds of signal have different energies. When the signal varies rapidly (like during data bits), it has less time to ramp from a logical level to the other, resulting in a lower amplitude of the pulses. So, it is important where you adjust the volume: at the same volume level, the leader usually appears louder than the actual data.
- If necessary, use equalization. The computer signal is most energetic between 800 Hz and 3.5 KHz. Frequencies far out of this range are undesirable. If the tape contains a strong hiss, lower the highest frequencies; try enhancing the middles. Noise usually causes bad bit detections and wrong sync pulses. Never exceed in any direction; in most cases, keeping the default middle-settings is the best choice. This applies to preamplification levels too.
- Set the appropriate sampling rate. Go for at least 20 KHz for safety, but if the tape is rather old or damaged, increase it up to 44 KHz. Normal speed tapes converts even at 8 KHz, but we do not recommend to stay below 11 KHz.
- If the tape is noisy, try to enable the numeric filter with option "-f". This powerful filter reduces some undesired signals like DC-offset, 50Hz and noise. It is worth reading the [digital filter](#11-the-digital-filter) manual section to learn the great capabilities of this tool and how to get the most from it. When the tape is damaged and contains noise, the filter will make the difference!

Make sure that at least the leader is correctly detected.

Sometimes, trying with volume set to half way or full may help.

There are tapes that the real Spectrum reads successfully but MakeTZX doesn't convert. This happens when there are deep spikes somewhere in the signal, but the Spectrum isn't reading from the tape when they occur. In some cases, MakeTZX can correct the problem by itself because it can guess what to expect there; in the other cases, if you are skilled enough you can try to locate the exact point where this happens (you can read it in the logfile as soon as we put it back in v2.xx, or use MakeTZX v1.02) and use a sample editor to correct the problem.

**Choosing the right mode**

If the loader type of the program is one of those recognised by MakeTZX autodetect mode, then use switch "-a" and it will do everything for you; this is the most recommended way. In the other cases (and if autodetect should fail to identify the right mode), you should know what kind of loader you are in front of, since none of SpeedLocks, Alkatraz and the other commercial turbo loaders will convert in RAW mode. If you are not sure, then using DirectMode with switch "-rk" is a very good idea, since you may have to try some times before guessing the right subversion (unless it is an autodetected type or you can tell it clearly reading [section 7](#7-how-to-identify-a-custom-loader)).

For normal speed tapes you should always try normal mode first (switch "-ln"), since it is more precise than raw mode. However, some tapes may not convert in normal mode and work fine in raw mode; a typical case is when the tape runs at altered speed (too fast or too slow), so that the signal frequencies get somehow "shifted" and may fall out of the valid range assumed by normal mode. Again, ensure that the leader is detected. Note that the converse may happen too: if a normal speed tape does not convert in raw mode, try normal mode (perhaps this is more likely the case).

---

## 11. The digital filter

MakeTZX offers a powerful tool against noise disturbs that are frequently encountered in old tapes. It implements a very good quality, fully featured Digital Signal Processor (DSP) engine which offers great performance and results. It uses highly optimized assembler routines and takes full advantage of 3DNow! processors for extra speed. It has been especially thought for realtime conversion in DirectMode, but it can be enabled for any supported input file too (except CSW files who don't need any filtering).

Before we go on describing the filter and ho to use it, you may want to know something about signals and filters, so that you can understand the description better.

Of course, we do not present a formally correct exposition of Signal Theory here, but we try to explain in trivial words some basic concepts. If you are an expert already, you can skeep the next two or three subsections.

#### Signal spectrum and noise

Simply speaking, every signal can be considered as an infinite sum of oscillations at different frequencies and amplitudes; imagine these oscillations as "pure" frequency tones and focus over sine and cosine waveforms. The Fourier's theorem gives a formal proof for this fact and provides a method to compute the amplitudes of each sine (more exactly, they are complex exponentials but in practice we may think of them as sine and cosine functions). Spectrum analysers are just a way to compute this coefficients and show the results in a graph where the X-axis is frequency and Y-axis is the amplitude of the associated oscillation. If the amplitude associated to a certain frequency (say, 8000 Hz) is very low (if not zero), then we say that the considered frequency is not significant for (or not contained into) the signal that we are examining. Instead, the frequency with the highest amplitude in the plot is dominant and somehow characterizes the whole signal. For example, in the spectrum plot of an A-5 note played by a flute we will see that the 880 Hz component is dominant (has the highest amplitude); 880 Hz corresponds to the A-5 note. We will also notice that the peak is repeated several times every 880 Hz with globally decreasing amplitudes (at 1760, 2640, 3520 Hz and so on); all these scaled repetitions are called *harmonics* (multiples of the fundamental frequency) and they are very important too; their shapes contribute to make the difference between the sound of violin and a piano both playing the A-5 note. A "pure tone" (like a perfect sine function or the sound produced by a diapason) consists in the fundamental frequency only: there is only the first harmonic - at least in theory, in practice nothing is so perfect.

Higher frequencies are associated with high-pitched sounds, while lower freqs correspond to bass sounds.

Every kind of signal has a different spectrum, containing an unique mix of bass and treble tones and different harmonics. It's a sort of finger-print, for example voice recognition programs learn the "image" of your voice to recognize your words. The difference between a violin and a flute is reflected into two different shapes of their spectrums, with a different number of peaks at different frequencies and at different distances amongst them.

What is the highest frequency possibly contained into a signal? Well, theorically infinite. A simple signal like a square wave contains infinite harmonics (multiples of the fundamental frequency) with non-zero amplitudes (although they become closer to zero as the frequency grows). An important theorem (by Shannon) states that if you sample a time-continuous signal (for example with your soundcard) at a certain samplerate (say F Hertz), the sampled signal contains the original frequencies only up to F/2 Hertz. So, if you want to hear the highest harmonics of an orchestra at 22 KHz then you must sample at 44 KHz at least.

Unfortunately, real life signals always contain undesired frequency contributions that are not part of the "original" signal. They come from the outside world or are introduced by devices such as amplifiers, transmitters, converters and so on. We call *noise* every undesired external contribution which is not part of the original information carried by the signal; with this definition, also a ringing cellular phone at the theatre can be considered as noise! The bad bit about noise is that is cannot be competely eliminated from a signal in any way; it's not that we are not able to do this job, but it's rather a fundamental component of the real world. Noise and information are superimposed and there is no way to properly separate them, because noise is somewhat "unpredictable" and it cannot be subtracted.

Once we have identified the noise into a signal, we can only hope to attenuate it enough without affecting the signal itself beyond a certain limit. Sometimes, like in certain old tapes, the noise can be so loud that the intensity of its frequency contributions is comparable with those of the original signal, making it hardly interpretable. In this cases, it is absolutely necessary to use some tool to reduce such noise and make it a lot weaker than the actual signal.

#### Filters

Filters are "devices" (real or just mathematical) that operate over spectrums and transform them. Filters themselves are associated with a frequency response, that is they attenuate or amplify each frequency of the input signal in a way that depends from the frequency itself. To clarify, a certain filter could leave unmodified (that is with an amplification of 1.0) the amplitudes of all frequencies from 0 to 8000 Hz and then force to zero all the rest. A similar entity is called a *low-pass* filter because it allows only the lower frequencies to be present into the output signal; it will cut the beautiful crystal sound of a violin (frequencies over 10 KHz), but if it could amplify rather than just pass the low freqs, than it would enhance your favourite disco music with lots of percussions and bass. The frequency where the filter changes its behaviour is called cut-off frequency.

Similarly, a *high-pass* filter will cut the the lower frequencies and pass the highest ones, while a band pass filter will act like a lowpass and a highpass together: it will pass only the frequencies falling within a certain range (say 600 to 4100 Hz).

A *band-pass* filter consists in a low-pass and a high-pass combined together, so it allows the frequencies falling within a certain range.

An ideal filter has sharp square edges in correspondence of the cut-off frequencies. For example, the frequency response diagram of the perfect band-pass filter is a rectangle with the vertical edges located at the two cut-off frequencies; between these frequencies the amplification is 1.0 (does not modify the input), while it is 0.0 outside. The problem is that a similar thing is not physically realizable. Real life filters have "smooth" and continuous transitions between the various zones of different amplifications, without gaps. Of course, when we are interested in leaving only a certain range of frequencies and cutting out all the rest, we want the slopes of the transitions to be as steep as possible, in order to reach the best approximation of the ideal behaviour. This steepness is somewhat related to the order of the filter (the number of poles of the transfer function); we cannot explain further in this direction without facing complex mathematical arguments, so believe that the higher the order, the steeper (better) the slopes. A high order filter will be more selective than a low order one. For example, a first-order lowpass with a cut-off frequency of 1000Hz will show an asinthotic diagram that will start to decrease after 1000Hz with a slope of -20dB per decade (attenuation will be -20dB at 1000 * 10 = 10,000 Hz and -40dB at 10,000 * 10 = 100,000 Hz if it started from 0dB); for a second-order filter the slope is the double (-40dB) and so on. In the real world, it is not possible to have perfect square-edged filters.

It is important to note that the slope at the edges of a band-pass of order N is half the slope of a low-pass or a high-pass of the same order, so if you want a band-pass with the same filtering strenght of a low-pass of order 3 you must use a band-pass of order 6; to understand why this happens, imagine to split the order 6 as 3+3 and that a 3 goes for the embedded low-pass and the remaining 3 is for the high-pass.

#### Spectrum of the Spectrum

Now let's see what the spectrum of a ZX Spectrum tape looks like. Our computer produces only square waves at different frequencies and at different times, so the shape of the spectrum will be the generally the same, more or less shifted and scaled depending on the base frequency of the oscillation. However, for physical reasons the real signal recorded on the tape cannot be a perfect square wave, but it is rather a "smooth" sine, more or less squared. Now, if you recall what we said above about ideal filters, we could say that Nature doesn't really like square edges! Good electronic components produce sharper edges, so you can evaluate the goodness of your Spectrum plus your tape recorder combined together by examining the waveform of a recorded tape with a sample editor (assuming that you have an ideal soundcard, otherwise it will contribute to shape the signal too!). The sharper the edges, the better the quality. Incidentally, these considerations also affect the signal quality of a video card (sharp edges = finer details), the maximum clock speed for an integrated circuit (high clock speeds need high currents and so high power consumption) and a variety of other situations.

Ok, now that we don't expect real square waves from our computer, let's see what are the typical frequencies involved in ZX Spectrum recorded signals. At the moment we are interested into the fundamental frequencies only, disregarding of the harmonics. We will take into account the first three or four significative harmonics later.

The lowest frequency is the pilot tone, a "square" wave of about 808 Hz. The bit-0 of the standard ROM loader is at 2044 Hz (bit-1 is 1022 Hz) and the sync pulse is a short pulse at nearly 2450 Hz. Examining the spectrum of the pilot tone, we see that the louder frequency contribution is obviousely at 808 Hz (the fundamental) and that the successive 4 harmonics (at 1616, 2424, 3232 and 4048 Hz respectively) are quite evident too; however, the fourth harmonic is about 40dB weaker than the fundamental, so it is not really determinant but we will try to keep at least the second harmonic.

The highest frequencies are shown by the fastest turbo loaders. Alkatraz and Zetaload are good examples; they run at 3100 Hz for the bit-0 waves, and this is nearly the maximum limit for a tape recording. Considering the third harmonic, a reasonable higher cut-off frequency is 3100 * 3 = 9300 Hz which we will round to 10 KHz. Remembering the Shannon theorem, for a good sample quality of a turbo tape we must sample at least at 20 KHz if we accept to keep the very first harmonics only, otherwise we have to increase the samplerate to 44 KHz to include also the 7th harmonic. Now you see why a decent samplerate is so important!

#### Tape noise

An old tape frequently presents two kinds of problems. The most important is *hiss*, the high-pitched background noise that you clearly hear instead of silence. It is characterized by high-frequency contributions in the far right of the spectrum, so it can be attenuated with a lowpass filter. The second disturb comes from your electric wiring where the 50 Hz AC that powers your Spectrum and your tape recorder comes. The DC used by electronic devices is produced from the common alternate current by electric circuits which cannot be perfect, so you should not be surprised to find a modest 50 Hz component into the Spectrum signal, together with its first three or four harmonics (and more if you have an economic tape recorder). Although not so harmful as hiss, 50 Hz is an undesired presence too, so why not try to eliminate it?

Another frequently encountered problem is the *DC-offset*, i.e. a translation of the signal. Normally, the signal is centered around the zero of the scale (mid-scale) and it looks symmetrical respect to it; this means that the mean value is zero and that the duration of positive and negative semipulses is the same. Sometimes, instead, the signal appears traslated upward or downward by a certain quantity: it's not centered at middle-scale anymore and its mean value becomes sensibly different from zero. The amount of this translation is the DC-offset and it can measured with some sound editor like Cooledit. The DC-offset disturb may come from the electronics of your tape recorder or, more frequently, it is recorded onto the tape itself (factory-originated). The DC-offset can be considered as a 0 Hz oscillation (DC) and so it can be eliminated with a band-pass or high-pass filter.

As we said earlier, we would like to keep only those frequency components actively involved into the computer signal and discard all the others. We can think about a bandpass filter and choose the highest and the lowest Spectrum frequencies discussed above as cut-off frequencies; remember to take into account the harmonics.

Note that the filter modifies not only the noise components, but it affects also some harmonics of the original signal too, because they are superimposed. So, an aggressive approach to noise suppression can distort the signal excessively and make the situation even worse. For this reason, it's not a good idea to specify big orders for the filter; usually, a value of four or six can be considered the top limit before fatal signal distortion.

#### MakeTZX's filter

MakeTZX has an integrated DSP engine which can realize lowpass, highpass and bandpass filters of any order up to a maximum of 16. The order of a bandpass filter is automatically forced to be an even number. The user can select all the parameters including the cut-off frequencies. The default settings define a bandpass filter of the 2nd order with cut-off frequencies of 600 and 4100 Hz, so it is like a 1st order low-pass at 4100 Hz and a 1st order high pass at 600 Hz combined together. Besides, the user can choose between two prototypes of the filter. The former is the Butterworth model (assumed by default) which offers a good frequency diagram with a flat response over the allowed band and steep edges. The latter is the Chebyshev model which has steeper edges than Butterworth but the central region of the diagram is not flat and presents a certain ripple, so it distorts the signal a bit. The amount of the ripple can be adjusted, but the lower the ripple (for a flat diagram), the less steep the edges. There is plenty of parameters to play with if you believe you need a special filtering process different from the default one. We think that it is flexible enough to allow very efficient noise removal for most situations, so take some time to experiment with it and the best results are guaranteed!

Once you know the basic working of filters, feel free to experiment with the settings of MakeTZX's filter. The default settings should work in many cases, but sometimes the best results can only be found with repeated experiments. You have a powerful tool to play with, and it is certainly able to solve the most difficult cases if used correctly.

On the performance side, the DSP engine has been totally written in pure assembler and heavily optimized by hand both for the standard x87 FPU and for 3DNow! technology. MakeTZX automatically detects a 3DNow! compatible processor and uses the accelerated version of the routines; the 3DNow! code is about 4 times faster than the optimized FPU version and beats also a Pentium II at the same clock frequency. The ANSI-C version (used for non-x86 platforms) is 9 times slower than 3DNow! and 2 times slower than the current asm x87 FPU code.

Important note: the 3DNow! instructions work with single precision floating point numbers (floats, 32 bit), but the FPU version has been implemented to work in double precision (doubles, 64 bit). Floats are only accurate up to the 7th decimal digit and for complex mathematical considerations their use for high order filters leads to numeric instability (the filter becomes unstable because the coefficients cannot be represented with a sufficient precision); in these cases, 3DNow! acceleration is automatically disabled. For the same reason, the maximum order of a filter is limited to 16; beyond this value, also double precision numbers are not accurate enough.

#### Some hints

Finally, here are some general tips to keep in mind:

- Choose the right filter type. To eliminate hiss you need a low-pass, for DC-offset and 50 Hz a band-pass is ok. In general, the band-pass filter covers all the problems but in some cases you may prefer a low-pass. High-pass isn't usually desirable for ZX Spectrum tapes.
- Don't exceed with the filter order. Most times, using orders higher than 4 is useless and may introduce further distortion. Remeber that a band-pass of order N has the same strenght of a low-pass (or a high-pass) of order N/2 and vice versa.
- If you don't know how to guess the cut-off frequency settings, start with with the default band and then change the upper and lower limits gradually (try increasing and decreasing).
- Make sure that the highest cut-off frequency stays below half the sampling frequency.
- In terms of speed, the filter works best in realtime conversion (DirectMode), where asm optimizations can run at full throttle. When applied to sample files, it can slow down the conversion process, especially for VOC and IFF files (less for WAVs). However, working on the original samples allows you to perform independent filtering processes to find the optimal settings. Do not filter an already filtered file, such as the realtime dump file saved by switch "-rk" when the filter was enabled during recording.
- If you don't use DirectMode, use genuine ("raw") sample files of the tape, directly produced from your sampler program (without any signal processing option) and not generated by some conversion (such as TZX -> VOC, TAP -> VOC and similar). Don't pre-process VOC files through other digital filters such as READSB's one, since this prevents any other filtering to be effective.

---

## 12. Graphic interface: MakeTZX WinGUI

WinGUI is a graphical user interface for MakeTZX running under Windows 95/98. It provides intuitive access to all MakeTZX functions simply with mouse operations, hiding the native DOS command line interface completely.

With WinGUI installed, you don't need anymore to start MakeTZX from the command line, although you can still do so if you prefer. MakeTZX remains a console program, but WinGUI adds new features to it.

#### Installation

To run WinGUI you need Windows 95 or 98 and MakeTZX v2.30 or above. It can be used with earlier versions of MakeTZX too (v2.00 minimum), but some of the newer options could not be supported by the old executables, causing a syntax error.

To install WinGUI, download the WinGUI package from MakeTZX's homepage and unpack it into the same directory where MakeTZX resides. You can also put it into another directory, but then you will need to configure the file locations from the *Options/Configure paths* menu. WinGUI may create a small configuration file (INI) in the same directory; do not edit it manually.

#### How to use it

When you launch WinGUI, you are presented with a control panel containing all the controls of MakeTZX. If you are already familiar with MakeTZX, you should immediately understand the meaning of the controls since they reflect the various command line switches of MakeTZX very closely.

First of all, adjust the settings to match your requirements; for example, select the loader type, configure the DirectMode and digital filter options if desired and specify the input and/or the output files (as required). The Browse button near the input filename field will bring you a handy file selector to choose the file to convert.

Once you are ready with the settings, you may press the Start button and MakeTZX will be launched. This will open a console box automatically, where MakeTZX will produce the usual messages. When the conversion has terminated and you have examined the results, press any key on the console box to close it automatically. Note that while a conversion is in progress, you may still operate with the control panel of WinGUI and launch other conversions; in this way you can run several instances of MakeTZX in parallel, provided that only one realtime DirectMode conversion at a time may exist.

#### Define your own custom loader types

If you wish, you can now define your own set of custom schemes to extend MakeTZX's built-in loader types. The new loaders created by you are included as additional items into MakeTZX's supported types list, so you can use them just like any other predefined type. The user-defined loaders editor is accessible from the main menu with the *Options/Configure loaders* button and allows easy insert/modify/delete of your schemes (profiles). To create a new loader type, enter its name into the "Profile name" field and then fill in the various parameters (sync pulses and bit lengths, all expressed in T-states); the "Skip" field is the number of normal-speed blocks to skip before the first turbo block (e.g. skip=2 if there's a only a BASIC loader, skip=4 for one BASIC + one CODE, etc. -- this value is automatically added to the Skip parameter specified into the "Decoder Options" frame from the main control panel). Finally, press "Add" to append the newly defined type to the existing list. To remove a loader, just highlight it and then press "Delete". If you wish to edit an existing scheme, select it and change the parameters, then press "Apply" to make them permanent. When you have finished, press "Done" to close the editor and save the preferences. The loaders database is stored into WinGUI initialization file, so it is preserved amongst the various runs.

#### TZX Viewer

The GUI has a little TZX viewer which can be used to examine the contents of a TZX file. To show it, just press the corresponding button in the main screen. It is not intended as a fully featured TZX manager, so it has very limited capabilites. You can use it to check any TZX file, for example those produced by MakeTZX itself. Its usage is very simple, just load the TZX in using the "Open..." button. There are 5 columns showing some info about the various blocks; the details presented should be self-explanatory. The CRC column shows "OK" if the block has a valid ROM-CRC; if the block has errors, it prints the current CRC value and, in brackets, the expected value. Note that for some protection schemes (for example, Alkatraz and Speedlock sub-blocks) the CRC column is not significant and must be ignored (because they use different ways to compute checksums). The viewer can be used simultaneously with the rest of the GUI and the various MakeTZX instances.

#### Uninstall

To completely remove MakeTZX WinGUI from your system, simply delete the MTZXWGUI.EXE executable; if present, delete the MTZXWGUI.INI configuration file which resides in the same directory where the executable is.

---

## 13. FAQ (Frequently Asked Questions)

Here are some common questions that we have been directly or indirectly asked up to now. If you cannot find the answer to your question here, don't hesitate to write to us.

#### 1. Why did you invent CSW?

Well, CSW was created because we needed it. Each time that MakeTZX is modified, we test it with 57 sample files of different kinds (and, at the moment, it hits 56 of them, by the way). Imagine that our hard disks are full of rubbish and that 57 VOCs take *a lot* of megabytes. That's how CSW was born: we needed to store the samples in a compact form, preventing them to fill up the hard disk. At first, we used it only for internal purposes, but later we thought that it might have been useful for someone else too... and this is why we made the mistake to release it to the public! :-)

Anyway, if you don't like CSW, or you don't think it may be useful to you, simply don't use it! We didn't want to impose a new format to anyone! :-)

#### 2. Zipped CSWs are smaller than zipped VOCs?

A packed CSW is always smaller than the corresponding packed VOC, unless the VOC is a binary VOC, that is produced with READSB and digitally filtered; in this case the compression ratios are similar.

#### 3. When I convert my CSWs back to VOC, some of them don't load anymore. Why?

The original VOC file is processed through MakeTZX's internal digital filter to produce the CSW and it means that when you convert back to VOC you will not get the very same samples; this may happen even if the original VOC was produced by READSB. Note that this is not a crime, but it is often an improvement, instead. Sometimes, however, the signal is recorded under critical conditions and while the Spectrum may not be affected by a certain error (because it is not INning in that moment, for example), MakeTZX's filter (included in CSW.EXE) cannot avoid it. When this happens, the new VOC produced from the "faulty" CSW will contain the error in much greater evidence than in the original one, thus causing a tape loading error. Note that it is possible, however, that MakeTZX converts the "faulty" CSW successfully.

In general, you should consider such weird cases as warnings about the bad quality of the original VOC.

#### 4. When I start DirectMode (MSDOS), MakeTZX pauses for about two seconds and then exits saying "FATAL ERROR: IRQ #n LOCKED!" or something like that.

This question is fully covered [here](#soundcard-irq-lock).

#### 5. When I start MakeTZX in DirectMode, it prints "Press any key when done with volume meters" but the system hangs.

This is a problem with the soundcard autoinit DMA mode. You may try to use switch "-rc" ([compatibily mode](#soundblaster-compatibility-mode)) which can make things work when you have a SB compatible soundcard. On Windows Sound System cards, "-rc" have effect only if you specify also "-rb" and the soundcard is capable of simultaneous Soundblaster emulation. Please read the [DirectMode](#4-soundcard-directmode-realtime) section carefully, which contains some info about soundcard compatibility issues.

#### 6. During realtime conversion in DirectMode, sometimes MakeTZX says "DMA overrun".

This happens when MakeTZX processes the soundcard data too slowly. Probably, accessing your hard disk requires too much time because it's busy (under a multitasking environment) or you are not using a disk caching software. If you are running MakeTZX under plain MSDOS, please always load SMARTDRV.EXE or a similar program because it strongly reduces the disk latency. Under Windows 9x you don't need anything (there's the integraded VDISK).

If you are using the digital filter on a very slow machine, try to disable it.

A very remote possibility is that your CPU is too slow and MakeTZX gets very busy with Advanced Error Correction, but it's almost impossible...

#### 7. When I try to use DirectMode, MakeTZX hangs. Why?

Probably, autoinit DMA isn't working properly with your soundcard. Read the compatibility issues described in section 4. If MakeTZX reports a DSP version greater or equal than v2.02, typically v3.01, probably you have a SB compatible which emulates the SBPro playback functions but cannot record. Many ESS chipsets do so, including the brand new Soundblaster PCI128 which has an Ensoniq chipset.

You may try with [Soundblaster compatibility mode](#soundblaster-compatibility-mode) (switch -rc); in the other cases, check that the BLASTER or WSSCFG environment string reports the correct values for IRQ and DMA. Use a diagnostic program if necessary.

#### 8. In DirectMode, I play the tape but I don't hear the noise from the speakers.

First of all, ensure that MakeTZX hears the signal and does the conversion. Look for "Loading pilot" and similar messages. If this does not happen, check the soundcard mixer (using the configuration utility which comes with the soundcard) and be sure that the proper input source is enabled, otherwise read section 4 about the compatibility issues. If MakeTZX converts, instead, check the mixer to enable the output. On Windows Sound System cards, use the configuration utility which comes with the soundcard to adjust the volume settings and select the proper input source (typically LINE). If nothing happens, tell us the model and DSP version of your soundcard.

#### 9. Is the source code of MakeTZX available?

We are not yet going to release the sources of version 2.xx, but the old v1.02.1 code is freely available for download at our site. Please read carefully the enclosed documentation.

#### 10. MakeTZX says that all is OK, but the resulting TZX doesn't work.

Most probably this is due to a bad sync pulse which is the most difficult thing to detect in a Spectrum tape sample file. If a sync pulse hasn't been correctly detected, MakeTZX would report a checksum error on 95% of times; the remaining 5% is when MakeTZX detects a wrong sync pulse but the checksum is still good, so MakeTZX saves this block without processing it twice, since it has been "loaded ok". If you look at the resulting TZX with a proper utility (or a simple hex editor if you dare :-) ), you'll find that the block that causes the loading error has the flag byte value divided or multiplied by 2 modulus 256: this is because the bad detection subtracts a bit to the flag.

#### 11. MakeTZX prints "done." but it doesn't produce any block.

First of all, ensure that the sample file to convert contains valid ZX Spectrum sounds, of course. Check if MakeTZX detects at least a pilot tone ("loading pilot" should appear at some moments). If not, try to repeat the conversion enabling the digital filter (it could be due to DC-offset, see the Filter section of this manual). If you still have no success, try to specify a particular loader type such ROM timings or some commercial turbo. Check with a sound editor (like CoolEdit) if the waveform presents serious problems (excessive distortion, volume, etc). If you can, please send us a piece or the entire sample file; if you want to send all the file, you can compress it with CSW.EXE and the send us the zipped CSW file obtained. Contact us and describe exactly the problem, reporting the command line used and the operations done.

#### 12. What are TZX files?

The TZX is a digital tape format that allows a faithful representation of the original tape. Unlike the famous TAP, TZX files describe the physical signal structure of the cassette, meaning that the loading process will be reproduced exactly preserving the behaviour of turbo loaders and custom loading schemes. This includes the fancy loaders found in commercial games and many demos. You won't notice the difference between the real tape and its TZX version. You can find the tecnical specification of the TZX format in our [tech page](http://www.ramsoft.bbk.org/tech.html) or at the official site [WOS](http://www.worldofspectrum.org).

#### 13. What are OUT files?

OUT files are the final solution to conversion problems. They are log files produced by emulators such as Z80. Click [here](#9-how-to-use-out-files) for a description and instructions on how to use them.

#### 14. My VOC loads and runs into the emulator, but MakeTZX doesn't convert it in any way (filter, etc).

This isn't a problem anymore. MakeTZX is able to convert anything which loads in the emulator using the [OUT files technique](#9-how-to-use-out-files), which is described in this manual.

---

## 14. Revision history

The latest version has been released on August 1st 2003.

#### New in version 2.33:

- **Excelerator and InjectaLoad support.** Two new decoding schemes used by CRL in games such as Loads of Midnight, Last Mohican, Jack the Ripper (Excelerator) and Book of the Dead, Ninja Hamster, 3D Game Maker (Injectaload). Both of them are autodetected.
- **MultiLoad enhancements.** Formerly known as 'Activision', the **MultiLoad** (-lm) handling has been enhanced to support some variants and now it can be autodetected also.
- **CSW 2.00 support.** The new revision of the CSW fileformat offers vastly improved compression ratio and support for higher samplerates. Check the documentation of the CSW utility for more information.
- **Raw decoding bugfixes.** Some silly bugs caused standard speed blocks to be saved as turbo blocks with slightly incorrect timings.
- **Windows and Linux DirectMode bugfixes.** Corrected a nasty problem which caused occasional loss of samples, thus making DirectMode conversion unreliable under these operating systems. The bug was not present in MSDOS DirectMode.
- **Support for stereo and 16-bit WAV files.** We included this just for compatibility, since to ensure a good conversion quality there's no need for them and we still recommend using plain 8-bit mono samples. In this release the digital filter (-f) can be applied to 8-bit mono WAVs only.
- **Various changes and bugfixes.** By popular demand, the beautifier option (-b) has been disabled. Several bugs removed in various points of the code.

#### New in version 2.31:

- **PowerLoad support.** A couple of new custom loading schemes: PowerLoad (-lp) is used in games such as Arena and it can be autodetected.
- **Softlock bugfix.** The Softlock decoder now works properly and the scheme is now autodetected.
- **Enhanced Linux DirectMode.** The Linux realtime sampler now supports also 16-bit and signed samples if the classic 8-bit unsigned mode is not available (e.g. with the SB Live! OSD driver).
- **General bugfixes.** Several adjustments, including a fix to the samples pipeline which affects Speedlock 1 decoding (and possibly others).

#### New in version 2.30:

- **High-quality digital filter with 3DNow! optimizations.** The previous numeric filter has been replaced with a brand new, fully configurable DSP engine developed by us for MakeTZX; unlike its predecessor, the new filter can be used also for sample files. The routines are hand-written in 100% assembler, optimized both for the standard FPU and for AMD 3DNow! technology; the 3DNow! code runs 5 times faster than the FPU version. The DSP supports a complete set of numeric filter types and the expert user is given the ability to take full control over the various parameters. A great help against noise disturbs in old tapes (including DC-offset)! The manual has been updated with a new section explaining the basic concepts behind filtering in simple words.
- **100% error-free conversion through Z80 OUT files.** A revolutionary approach which guarantees mathematical success of conversions, thanks to our brand new technique which allows MakeTZX to process runtime information produced by emulators (OUT files). The result is that MakeTZX successfully converts any tape that loads and runs into the emulator and this simply means the end of many problems! OUT files are currently produced by Gerton Lunter's Z80 emulator and they are just a new supported input file format, in fact. Please read the special section of this manual dedicated to OUT files to learn how this technique works.
- **New custom loaders support.** The **Activision** type (-lc) covers games like Dynamite Dux, Ninja Spirit, Pacland and Timescanner, which didn't convert before. For italian users, the Special Program and Playgames tape magazines are supported with the **Biturbo** type (group -lg), which is also fully autodetected (all variants). As usual, MakeTZX prints detailed information about the low-level aspects.
- **Crystal/Analog codec DirectMode driver.** Added support for Windows Sound System (WSS) compatible soundcards, i.e. all boards equipped with a Crystal/Analog codec or equivalent, such as the CS4231 family, AD1848 and many others. Specific support for the Ensoniq Soundscape cards. The WSS codec is autodetected and used before SoundBlaster. You must have the WSSCFG (or SNDSCAPE) environment string set (e.g. set WSSCFG=A530 I7 D1 or SNDSCAPE=C:\WINDOWS). The behaviour is exactly the same of the SoundBlaster driver. Tested on OPTi 82C924 Audio 16 PnP soundcard (CS4231A). This driver has always been there since v2.11, but nobody told us that it worked! :)
- **Linux DirectMode.** Now it is possible to convert in realtime also on Linux (and possibly all Unix-like systems); MakeTZX uses the operating system sound driver, so make sure that your kernel is properly configured to support your soundcard and that sound is enabled. The device /dev/dsp must be present.
- **Experimental Windows DirectMode.** When coupled with the new graphic interface WinGUI and run under Windows, MakeTZX is able to record sounds using the Windows audio driver instead of the built-in Soundblaster driver so that it should work with any soundcard. Note that this feature is still very experimental and may not work correctly on some systems, so use it carefully at your own risk! Please read the dedicated section in the manual, first.
- **TZX beautifier.** This new function ("-b") rounds the pauses of the blocks to "nice" values, improving the quality of the TZX file. The rounding is done intelligently, taking into account several parameters.
- **User-defined custom loaders.** With WinGUI you can easily create your own schemes and extend MakeTZX's built-in loaders set. The new loaders can be selected and used just like any other predefined type. The loader characterization is quite simple at the moment, but we can extend it to describe signals of any complexity in a future release, if necessary. WinGUI comes with a configuration file containing the definitions for **Sinclair User** and **Your Sinclair** turbo loaders. Read the WinGUI section of the manual for more info.
- **Bugfixes and code optimizations.** We have removed a lot of bugs and added several important improvements in many sections of the code (Alkatraz, AEC, file processing, autodetect and more), although most of them are not directly visible to the user. Memory consumption has been reduced and overall speed increased.
- **Manual additions.** The manual has been significantly extended with more details, examples and explanations in all sections. Check it out!

#### New in version 2.21:

- **Paul Owens protection system** support (Batman the Movie, Operation Thunderbolt, The Untouchables, Chase HQ, Rainbow Islands, Red Heat and similar). This new type can also be autodetected with switch "-a". Previousely, it was possible to convert Paul Owen protected games both in RAW and NORMAL mode that produced fully working TZX files, but the signal timings were slightly incorrect because the relation bit1 = 2*bit0 was assumed for the pulse durations, resulting in a reprouction that was not 100% faithful to the original.
- **VOC loader bugfix.** The block ID 9 (GoldWave extensions) was handled incorrectly.

#### New in version 2.20:

- **Loader type autodetect.** MakeTZX can automatically recognize Alkatraz, Speedlock 1/2/7, Bleepload and Zetaload encoded programs. Simply specify switch "-a" to enable autodetect, and forget about "-l" parameters for the known types. MakeTZX will enter the necessary mode by itself as soon as it detects a specific loader.
- **DirectMode lowpass filter.** For altered tapes you may try option "-rf" which turns on a band-pass filter to reduce spikes and distortions. Works only for DirectMode realtime conversion. Requires extra CPU power, so if you have a slow processor and start getting "DMA overrun" messages, turn it off.

#### New in version 2.11:

- **Improved SoundBlaster support.** Added switch "-rc" to enable SB compatible mode which may allow DirectMode to work with some compatibles.
- **DirectMode stability check.** On startup, MakeTZX performs a preliminary test to verify that the realtime sampling driver works with your soundcard (see IRQ LOCK). This prevents system crashes in many cases. If the test is not passed, MakeTZX exits after two seconds and reports an error message.
- **TZX version bug removed.** Yes, again! It was defeated in v1.02 and suddenly reappeared in v2.xx, causing TZX files to be saved as version 10.1 instead of 1.10 :-(

  Hopefully fixed forever, an antidote has been synthetized to prevent future infections.
- **General bugfixes.** These include the removal of a stupid typo introduced in v2.10 sources that caused occasional crashes, plus some memory management improvements to avoid other potential problems.

#### New in version 2.10:

- **Partial "tape loading error" message removal.** The message is now displayed only on critical situations, i.e. when handling particular loaders that need to desume parameters from the loaded data (e.g. Speedlock)
- **Advanced Error Correction.** The existing AEC routines have been significantly enhanced and are now capable of bit error prediction and recovery (AEC level 2), that is they can detect and repair some bits that have completely changed state due to signal noise! Thanks to that, MakeTZX now converts many critical tapes such as "WCL.VOC" on the fly. A higher level AEC, working for more complex cases, is currently being tested and will be available soon.
- **Improved pulses detection.** Besides AEC-2 (which operates at the abstract level), we have also refined the physical edge detection code: nasty ripples are now eliminated. "Temple of Terror" is back to life!
- **Better sync pulse and data frequency estimation.**
- **Hybrid Speedlock 1/2 support.** A new Speedlock type halfway between type 1 and 2 (e.g. Highlander). It is supported by MakeTZX as a Speedlock 2. Hopefully the last variant out there... Improved Speedlock decoding routines.
- **Block status indication.** MakeTZX prints a single-character status indication for each decoded block, just after the filename. Read the [related manual section](#6-understanding-maketzxs-messages) for the details.
- **IFF support.** Now MakeTZX reads Amiga IFF/8SVX files directly.
- **Better DirectMode.** We have corrected some small problems that occurred sometimes when stopping the realtime conversion. The vu-meter is shown again during pauses. Sample rate is now set using low resolution also on SB16s, until someone tells us the internal rounding precision of DSP commands 0x41 / 0x42.
- **Minor bugfixes.** Nothing special here...

#### New in version 2.01:

- **VOC sample rate bugfix.** Now the sample rate of VOC files over 22KHz is computed accordingly to the vast majority of other programs as default. The alternate method is used only when an extended block is encountered. To know which formula is used, read the type qualifier that is now printed after the version number in the "Creative Voice File ..." header line. If it says "STD", then MakeTZX has assumed the VOC to be a standard one (READSB and others); if "EXT" is shown, instead, the new formula is used (COOLEDIT, any more?).
- **Speedlock 5 decription tones bugfix.** The multiwave generation only worked for a sample rate of 31053 Hz (oops!) :-) Now "hysteria.voc" doesn't crash anymore.
- **Fixed sample rate setting in DirectMode.** There was a rounding error when setting the sample rate for DSP v3.xx and below. SB16 and above were not affected by this.
- **Fixed a bug in the integrity check for VOC files.** The cache was reloaded after each block found in the file: this caused the processing to be quite long.
- **Empty TZX are now removed.** If MakeTZX doesn't produce any valid block, the empty TZX will be automatically deleted on exit.
- **Samples can be saved in WAV format.** When using DirectMode, you can now save to disk the sample file generated by the built-in sampler.
- **Added "fast pilots" detection in normal mode.** This is useful when converting programs that start with normal blocks followed by blocks with normal timings but very short pilots like Bleepload or Alkatraz ones. An example of such programs is given by earlier LERM Tape Utilities.
- **File name guessing routine fixed.** It only worked with '.VOC' and '.CSW' extensions.

#### New in version 2.00:

- **Totally rewritten C++ engine working in a 32-bit environment.** The new code is also easily portable to any platform.
- **Added SoundBlaster line-in sampler for realtime conversion.** Featuring programmed recording and built-in vu-meter plus clip-o-meter to get the best signal out of your cassette player.
- **New command line syntax.** Introduced shifted switches for better grouping.
- **Support for Microsoft PCM WAV and CSW 1.0**
- **Checksum calculation for first block of Alkatraz-encoded programs**
- **Fixed-length recovery bugfix.** Ambiguous pulses are now detected and corrected even on known-length blocks (like Speedlock 5 or greater sub-blocks)
- **Bad blocks bugfix.** Fixed problem with data recovering routine: the wrong block was saved in the TZX instead of the good one
- **Fixed problem with decoding engine:** if a normal block has a header and the actual length is different from the one specified in the header it would have reported an error (tested on "Skate or die")
- **Enhanced internal cache managing:** reduced the amount of read/write operations involving direct disk access.
- **The log file option has been (temporarily) removed.** Will be reintroduced in a future version.

#### New in version 1.02:

- TZX version bugfix (files were saved with version 10.1 instead of 1.10)
- Enhanced sync pulse detection

#### New in version 1.01:

- Added handling of VOC IDs 3,4 and 5 (thanks to Andy Schraepel)
- Default TZX revision used is now 1.10
- Enhanced decoding for extremely low sample rates (4 kHz)

---

## 15. Known limits and bugs

Please don't hesitate to contact us for any problem not covered in this manual. If you can, send also the file(s) that MakeTZX fails to convert (even in CSW format, which compresses well).

- When a "Tape Loading Error" is encountered, MakeTZX stops the conversion, ignoring all the following blocks. This does not make much sense, especially for preprogrammed recording in DirectMode. We will remove this limitation as soon as possible.
- MakeTZX can't handle VOCs that use repetition (IDs 6 and 7).
- Optimal VOC sampling frequency is greater or equal than 20000 Hz. However the program may work even with lower sample rates and warns you if the sample rate is under 11025 Hz.
- RAW mode may not work with very low sample rates.
- Bad sync correction works partially with programs encoded with ZetaLoad, Softlock and Speedlock 2 or above (only the first turbo block can be recovered at the moment)
- Loading check for Alkatraz blocks other than the first one is not performed (and most probably never will)
- Data blocks with an incomplete last byte are not allowed (except for Speedlock and ZetaLoad flags) so that byte remains unsaved.
- The WAV file obtained in DirectMode is 4096 bytes longer than it should be. This seems not to affect anything, however we'll fix it as soon as possible.
- Autodetect mode doesn't recognize some Speedlock 1 programs (e.g. Daley Thompson's Decathlon). At the moment it doesn't support Speedlock 3/4/5/6.

---

## 16. Future enhancements

The next-generation MakeTZX engine version 3 is now embedded into RealX, our brand new emulator which is the successor of RealSpectrum. The new MakeTZX works along the emulated Spectrum and it is capable of converting every tape which loads into the emulator. It is also associated with a fully featured tape editor.

Beside that, these are some enhancements we wish to include soon in MakeTZX (in no particular order).

- Remove the tape loading error stop limitation at all.
- Remove all the new bugs that we have introduced with previous bugfixes... :-)
- Re-enable the logfile generation.

---

## 17. Credits and contact info

MakeTZX is a program © 1998-2003 RAMSOFT. It comes with ABSOLUTELY NO WARRANTY of any kind; the authors are not liable for any damage or information loss caused directly or indirectly by the use of this program.

MakeTZX can be freely distributed at the conditions that no money is required and the original archive and its contents are not modified. If you want to include MakeTZX into any software collection distributed on CDROM or any other electronic media, you are allowed to do it at the above conditions, but please inform us.

The **[latest versions of MakeTZX](http://www.ramsoft.bbk.org/maketzx.html)** and related stuff (such as this document) are always available at our official web site **[World Wide Ramsoft](http://www.ramsoft.bbk.org)**

For comments/questions, write to:

- **[World Wide Ramsoft (ZX Spectrum demogroup)](mailto:ramsoft@bbk.org?Subject=MakeTZX)**

Please check the [contacts page](http://www.ramsoft.bbk.org/about.html) of our site for the latest email addresses.

---

MakeTZX and this manual are © 1998-2003 RAMSOFT - ZX Spectrum demogroup
