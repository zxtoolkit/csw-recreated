//! Z80 emulator "OUT" trace input: five-byte little-endian records (T-state
//! offset in a 1/200 s frame, 16-bit port, value written); a port whose low
//! byte is 0xFE ends a pulse of `tstates * rate / 3.5 MHz` samples.

use std::io::{Read, Seek, SeekFrom};

use crate::error::{Error, Result};
use crate::signal::Pulses;

/// ZX Spectrum CPU clock (T-states per second).
const Z80_CLOCK: u64 = 3_500_000;

/// Decode an OUT trace into pulses at the given sample rate, a block of
/// records at a time.
pub fn read<R: Read + Seek>(reader: &mut R, rate: u32) -> Result<(Pulses, u32, usize)> {
    let len = crate::source::len_of(reader)?;
    // An empty trace is read, not refused: it gets its description line, and
    // `main` then calls it useless.
    if len != 0 && !len.is_multiple_of(5) {
        return Err(Error::CheckedFatal(
            "bad size!".into(),
            "Input file is corrupted or in a wrong format".into(),
        ));
    }

    let mut pulses: Vec<u32> = Vec::new();
    let mut prev_ts: u32 = 0;
    let mut accum: u32 = 0;
    let mut port_events: u64 = 0;
    // Every gap the trace covers, in T-states, **wrapping at 32 bits**. The
    // playing-time line comes from this, not from the pulse sum, so a
    // backwards step that wraps it reads short.
    let mut t_total: u32 = 0;

    let mut buf = vec![0u8; 5 * 65_536];
    let mut flagged = false;
    let first_end = walk(reader, &mut buf, len, |rec| {
        flagged = apply_record(
            rec,
            rate,
            &mut pulses,
            &mut prev_ts,
            &mut accum,
            &mut port_events,
            &mut t_total,
        );
        flagged
    })?;

    // One more pulse when the last record ended none: the end of the file is
    // tested only between pulses, so the walk reads on from the **start** of
    // the file, and the first write to port 0xFE there ends a final pulse
    // whose gap is that record's timestamp less the last one.
    if let Some(end) = first_end.filter(|_| !flagged) {
        let (mut spare_events, mut spare_total) = (0u64, 0u32);
        walk(reader, &mut buf, end + 5, |rec| {
            apply_record(
                rec,
                rate,
                &mut pulses,
                &mut prev_ts,
                &mut accum,
                &mut spare_events,
                &mut spare_total,
            )
        })?;
    }

    // **Fewer than four pulses and only the first is kept.** A single-pulse
    // trace stamps the initial polarity **low**, every other trace high. The
    // count reported is the writes to port 0xFE, not the pulses kept. A trace
    // with no pulses is "useless", which `main` raises after the description
    // line.
    let described = port_events as usize;
    let initial_high = pulses.len() != 1;
    if pulses.len() < 4 {
        pulses.truncate(1);
    }

    // Levels alternate from there. The count the header carries and the
    // console reports is at least one, whatever was recovered: a trace whose
    // writes all share one timestamp has no gap to convert and still declares
    // "1 pulses".
    let mut sig = Pulses::new(rate, pulses, initial_high);
    sig.declared = sig.declared.max(1);
    Ok((sig, t_total, described))
}

fn walk<R: Read + Seek>(
    reader: &mut R,
    buf: &mut [u8],
    len: u64,
    mut apply: impl FnMut(&[u8]) -> bool,
) -> Result<Option<u64>> {
    reader
        .seek(SeekFrom::Start(0))
        .map_err(|e| Error::Fatal(format!("Cannot read the input file: {e}").into()))?;
    let mut first_end = None;
    let mut pos = 0u64;
    while pos < len {
        let want = (buf.len() as u64).min(len - pos) as usize;
        reader
            .read_exact(&mut buf[..want])
            .map_err(|_| Error::Format("OUT file ends mid-record".into()))?;
        for rec in buf[..want].chunks_exact(5) {
            if apply(rec) && first_end.is_none() {
                first_end = Some(pos);
            }
            pos += 5;
        }
    }
    Ok(first_end)
}

/// One five-byte record, carrying the trace's state from record to record.
/// Returns whether this record ended a pulse.
///
/// The accumulator and the timestamp are **32-bit unsigned and wrap**: a
/// timestamp that steps back 1000 T-states gives a gap of 0x04C118CB samples,
/// and that pulse is written. Keep them unsigned -- a signed gap drops the
/// pulse and inverts every level after it. The widest gap converts to
/// 79,763,678 samples, so a length always fits its DWORD.
fn apply_record(
    rec: &[u8],
    rate: u32,
    pulses: &mut Vec<u32>,
    prev_ts: &mut u32,
    accum: &mut u32,
    port_events: &mut u64,
    t_total: &mut u32,
) -> bool {
    let word_a = rec[0] as u32 | ((rec[1] as u32) << 8);
    let word_b = rec[2] as u32 | ((rec[3] as u32) << 8);
    // rec[4] is unused

    // The description count is every record naming port 0xFE, whatever its
    // timestamp: a record the walk skips or takes for a wrap marker counts,
    // and a trace of nothing but those is not "useless".
    if word_b & 0xFF == 0xFE {
        *port_events += 1;
    }
    if word_a == 0xFFFE {
        return false;
    }
    if word_a == 0xFFFF {
        let gap = word_b.wrapping_sub(*prev_ts);
        *accum = accum.wrapping_add(gap);
        *t_total = t_total.wrapping_add(gap);
        *prev_ts = 0;
    } else if word_b & 0xFF == 0xFE {
        let gap = word_a.wrapping_sub(*prev_ts);
        *accum = accum.wrapping_add(gap);
        *t_total = t_total.wrapping_add(gap);
        *prev_ts = word_a;
        // Truncated per pulse, no fractional carry. 64-bit: the product
        // overflows 32 where the quotient does not.
        let samples = *accum as u64 * rate as u64 / Z80_CLOCK;
        if samples > 0 {
            pulses.push(samples as u32);
        }
        *accum = 0;
        return true;
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rec(a: u16, port: u8, val: u8) -> [u8; 5] {
        [a as u8, (a >> 8) as u8, port, val, 0]
    }

    #[test]
    fn the_widest_gap_fits_the_dword_that_holds_it() {
        let mut data = Vec::new();
        data.extend_from_slice(&rec(0xFFFD, 0xFE, 0));
        data.extend_from_slice(&rec(0xFFFF, 0x00, 0x00));
        data.extend_from_slice(&rec(0xFFFF, 0xFC, 0xFF));
        data.extend_from_slice(&rec(0x0000, 0xFE, 0));
        data.extend_from_slice(&rec(1000, 0xFE, 0));
        data.extend_from_slice(&rec(2000, 0xFE, 0));
        let (p, _, described) = read(&mut std::io::Cursor::new(&data), 65_000).unwrap();
        assert_eq!(p.pulses, vec![1217, 79_763_678, 18, 18]);
        assert_eq!(described, 4);
    }

    /// A backwards timestamp wraps the 32-bit accumulator, and the enormous
    /// pulse that results is written, not dropped.
    #[test]
    fn a_backwards_timestamp_wraps_instead_of_going_negative() {
        let forward: Vec<u8> = [1000u16, 3000, 5000, 7000, 9000]
            .iter()
            .flat_map(|&t| rec(t, 0xFE, 0))
            .collect();
        let mut c = std::io::Cursor::new(forward);
        let (p, _, _) = read(&mut c, 65000).unwrap();
        assert_eq!(p.pulses, vec![18, 37, 37, 37, 37]);

        // The third timestamp steps back 1000 T-states.
        let backward: Vec<u8> = [1000u16, 3000, 2000, 7000, 9000]
            .iter()
            .flat_map(|&t| rec(t, 0xFE, 0))
            .collect();
        let mut c = std::io::Cursor::new(backward);
        let (p, _, _) = read(&mut c, 65000).unwrap();
        assert_eq!(p.pulses, vec![18, 37, 0x04C1_18CB, 92, 37]);
    }

    #[test]
    fn decodes_edges_to_pulses() {
        // Four port-0xFE OUTs at T-states 1000, 2000, 3500, 4000: four is the
        // fewest that converts in full (see `short_traces_keep_only_the_first`).
        let mut data = Vec::new();
        data.extend_from_slice(&rec(1000, 0xFE, 0x10));
        data.extend_from_slice(&rec(2000, 0xFE, 0x00));
        data.extend_from_slice(&rec(3500, 0xFE, 0x10));
        data.extend_from_slice(&rec(4000, 0xFE, 0x00));
        // at 3.5 MHz, factor 1.0 => samples == T-states
        let (p, _, described) = read(&mut std::io::Cursor::new(&data), 3_500_000).unwrap();
        assert_eq!(p.rate, 3_500_000);
        assert_eq!(p.pulses, vec![1000, 1000, 1500, 500]);
        assert_eq!(described, 4);
        assert!(p.initial_high);
    }

    /// Fewer than four port writes and only the first pulse is kept, while
    /// the reported count is still the writes that were seen. A trace
    /// yielding one pulse also stamps the initial polarity low.
    #[test]
    fn short_traces_keep_only_the_first() {
        let mut three = Vec::new();
        three.extend_from_slice(&rec(1000, 0xFE, 0x10));
        three.extend_from_slice(&rec(3000, 0xFE, 0x00));
        three.extend_from_slice(&rec(6000, 0xFE, 0x10));
        let (p, _, described) = read(&mut std::io::Cursor::new(&three), 3_500_000).unwrap();
        assert_eq!(p.pulses, vec![1000]);
        assert_eq!(described, 3);
        assert!(p.initial_high);

        let one = rec(1000, 0xFE, 0x10);
        let (p, _, described) = read(&mut std::io::Cursor::new(&one), 3_500_000).unwrap();
        assert_eq!(p.pulses, vec![1000]);
        assert_eq!(described, 1);
        assert!(!p.initial_high, "a single write stamps polarity low");
    }

    #[test]
    fn wrap_marker_adds_time() {
        // OUT at 500, then a 0xFFFF wrap carrying 1000, then OUT at 200, and
        // two more so the trace is long enough to convert in full.
        // pulse1 = 500-0 = 500; wrap adds 1000-500=500 then prev=0; pulse2 = 200-0 + 500 = 700
        let mut data = Vec::new();
        data.extend_from_slice(&rec(500, 0xFE, 0x10));
        data.extend_from_slice(&[0xFF, 0xFF, 0xE8, 0x03, 0x00]); // word_b = 1000
        data.extend_from_slice(&rec(200, 0xFE, 0x00));
        data.extend_from_slice(&rec(400, 0xFE, 0x10));
        data.extend_from_slice(&rec(600, 0xFE, 0x00));
        let (p, t_total, _) = read(&mut std::io::Cursor::new(&data), 3_500_000).unwrap();
        assert_eq!(p.pulses, vec![500, 700, 200, 200]);
        // The T-state total covers the same span, wrap included.
        assert_eq!(t_total, 1600);
    }

    /// A trace that logs some other port has no pulses, and that is not an
    /// error here: `main` prints the description line, then the "useless"
    /// message.
    #[test]
    fn a_trace_of_another_port_has_no_pulses() {
        let data = rec(1000, 0x1F, 0x10);
        let (p, _, described) = read(&mut std::io::Cursor::new(&data), 44100).unwrap();
        assert!(p.pulses.is_empty());
        assert_eq!(described, 0);
    }

    #[test]
    fn rejects_bad_size() {
        assert!(read(&mut std::io::Cursor::new(&[0u8; 7][..]), 44100).is_err());
    }

    /// A trace whose last record ends no pulse runs past the end of the file
    /// and reads on from the start of it, growing one final pulse.
    #[test]
    fn a_trace_ending_mid_pulse_reads_on_from_the_start() {
        // Four writes 2000 T-states apart, then a record for another port.
        let mut trace: Vec<u8> = [0u16, 2000, 4000, 6000]
            .iter()
            .flat_map(|&t| rec(t, 0xFE, 0))
            .collect();
        trace.extend_from_slice(&rec(7000, 0x00, 0));
        let mut c = std::io::Cursor::new(trace.clone());
        let (p, _, described) = read(&mut c, 65000).unwrap();
        // Three pulses of 37 samples, then the gap from 6000 back to the
        // first record's timestamp of 0, wrapping at 32 bits.
        let wrapped = (0u32.wrapping_sub(6000)) as u64 * 65000 / Z80_CLOCK;
        assert_eq!(p.pulses, vec![37, 37, 37, wrapped as u32]);
        assert_eq!(described, 4);

        // The same trace ending on the write instead: no extra pulse -- and
        // with three left, the fewer-than-four rule keeps only the first.
        let mut c = std::io::Cursor::new(trace[..trace.len() - 5].to_vec());
        let (p, _, _) = read(&mut c, 65000).unwrap();
        assert_eq!(p.pulses, vec![37]);
    }

    /// The description count is every record naming port 0xFE, whatever its
    /// timestamp says -- a skipped 0xFFFE and a 0xFFFF wrap marker included.
    #[test]
    fn the_count_is_over_the_port_byte_alone() {
        let mut trace: Vec<u8> = Vec::new();
        trace.extend_from_slice(&rec(0, 0xFE, 0));
        trace.extend_from_slice(&rec(0xFFFE, 0xFE, 0)); // skipped, still counted
        trace.extend_from_slice(&rec(0xFFFF, 0xFE, 0x01)); // wrap marker, counted
        trace.extend_from_slice(&rec(2000, 0xFE, 0));
        let mut c = std::io::Cursor::new(trace);
        let (_, _, described) = read(&mut c, 65000).unwrap();
        assert_eq!(described, 4);
    }
}
