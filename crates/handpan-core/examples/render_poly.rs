//! Polyphonic demo: one coupled handpan, two independent sequencers running at
//! once (a slow low line + a faster upper line), with accented chord "impacts"
//! landing on top. Everything rings through the same instrument, so the fields
//! interact (shell intermodulation + strike-time coupling).
//! `cargo run -p handpan-core --example render_poly`.

use std::fs::File;
use std::io::{BufWriter, Write};

use handpan_core::{scale::Scale, Build, Handpan, PlayMode, Sequencer, Step};

const FS: f32 = 48_000.0;

fn main() -> std::io::Result<()> {
    let mut hp = Handpan::from_preset(FS, &Scale::DHijaz9, Build::Handpan, handpan_core::Size::Large);
    hp.set_air(0.20);
    hp.set_shell_nonlin(0.07); // subtle interaction; hard hits stay smooth
    let n = hp.note_count();
    let low = n / 2; // split point between the two registers

    // Sequence A — slow, wandering, lower register, soft.
    let mut a = Sequencer::new(n, 0xA11C_E5ED);
    a.set_mode(PlayMode::Wander);
    a.set_feel(0.20, 0.20);
    let clk_a = 0.85;

    // Sequence B — faster, up/down melodic, upper register.
    let mut b = Sequencer::new(n, 0xB0B0_5EED);
    b.set_mode(PlayMode::UpDown);
    b.set_feel(0.15, 0.05);
    let clk_b = 0.34;

    // Accented chord impacts, expanded into a soft hand-roll: each note of the
    // chord is staggered ~16 ms and slightly softer, so the transients don't
    // stack into a crunchy spike.
    let impact_chords: [(f32, &[usize]); 4] = [
        (3.6, &[0, 4, 7]),
        (8.6, &[0, 3, 6]),
        (13.6, &[1, 5, 8]),
        (18.6, &[0, 4, 7, 2]),
    ];
    let mut impacts: Vec<(f32, usize, f32)> = Vec::new();
    for (t0, fields) in impact_chords {
        for (k, &f) in fields.iter().enumerate() {
            impacts.push((t0 + k as f32 * 0.016, f, 0.82 - 0.05 * k as f32));
        }
    }
    impacts.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());

    let total_s = 28.0;
    let total_n = (total_s * FS) as usize;
    let stop_seq = total_s - 5.0; // let it ring out at the end

    let mut left = vec![0.0f32; total_n];
    let mut right = vec![0.0f32; total_n];
    let mut next_a = 0.0f32;
    let mut next_b = 1.0f32; // B enters a beat after A
    let mut imp_i = 0usize;

    for i in 0..total_n {
        let t = i as f32 / FS;

        if t >= next_a && t < stop_seq {
            if let Step::Strike { field, velocity } = a.clock(0.6) {
                hp.strike(field % low.max(1), velocity * 0.7); // low register, soft
            }
            next_a += clk_a;
        }
        if t >= next_b && t < stop_seq {
            if let Step::Strike { field, velocity } = b.clock(0.78) {
                hp.strike(low + field % (n - low).max(1), velocity); // upper register
            }
            next_b += clk_b;
        }

        // Impacts: staggered chord notes (a hand roll), sample-accurate.
        while imp_i < impacts.len() && t >= impacts[imp_i].0 {
            let (_, f, v) = impacts[imp_i];
            hp.strike(f.min(n - 1), v);
            imp_i += 1;
        }

        let (l, r) = hp.process();
        left[i] = l;
        right[i] = r;
    }

    let peak = left
        .iter()
        .chain(right.iter())
        .fold(0.0f32, |m, &x| m.max(x.abs()))
        .max(1e-9);
    write_wav_stereo("poly_demo.wav", &left, &right, 0.891 / peak)?;
    println!("Wrote poly_demo.wav: {total_s:.0}s, 2 sequences + {} impacts, peak {peak:.3}", impacts.len());
    Ok(())
}

fn write_wav_stereo(path: &str, left: &[f32], right: &[f32], gain: f32) -> std::io::Result<()> {
    let n = left.len();
    let (channels, bits, sr) = (2u16, 16u16, FS as u32);
    let block_align = channels * bits / 8;
    let data_len = (n as u32) * block_align as u32;
    let mut w = BufWriter::new(File::create(path)?);
    w.write_all(b"RIFF")?;
    w.write_all(&(36 + data_len).to_le_bytes())?;
    w.write_all(b"WAVE")?;
    w.write_all(b"fmt ")?;
    w.write_all(&16u32.to_le_bytes())?;
    w.write_all(&1u16.to_le_bytes())?;
    w.write_all(&channels.to_le_bytes())?;
    w.write_all(&sr.to_le_bytes())?;
    w.write_all(&(sr * block_align as u32).to_le_bytes())?;
    w.write_all(&block_align.to_le_bytes())?;
    w.write_all(&bits.to_le_bytes())?;
    w.write_all(b"data")?;
    w.write_all(&data_len.to_le_bytes())?;
    for i in 0..n {
        for &s in &[left[i], right[i]] {
            let q = ((s * gain).clamp(-1.0, 1.0) * 32767.0) as i16;
            w.write_all(&q.to_le_bytes())?;
        }
    }
    w.flush()
}
