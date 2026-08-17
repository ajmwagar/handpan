//! Render a montage across the mallet/bell family so the reusable core is
//! audible. `cargo run -p mallet-core --example render_mallets`.

use std::fs::File;
use std::io::{BufWriter, Write};

use mallet_core::{Instrument, Mallet};

const FS: f32 = 48_000.0;

fn main() -> std::io::Result<()> {
    let mut left = Vec::new();
    let mut right = Vec::new();
    let mut t0 = 0.0f32;

    // (instrument, section seconds, a little phrase to play)
    let sections: &[(Instrument, f32, &dyn Fn(&mut Vec<(f32, f32, f32)>))] = &[
        (Instrument::Marimba, 4.5, &|e| run(e, &[60, 62, 64, 65, 67, 69, 71, 72], 0.28, 0.8)),
        (Instrument::Xylophone, 3.5, &|e| run(e, &[72, 74, 76, 77, 79, 81], 0.2, 0.85)),
        (Instrument::Vibraphone, 6.0, &|e| chords(e, &[[57, 60, 64], [55, 59, 62], [53, 57, 60], [52, 55, 59]], 1.3, 0.7)),
        (Instrument::Glockenspiel, 3.5, &|e| run(e, &[84, 86, 88, 89, 91, 93], 0.18, 0.8)),
        (Instrument::MusicBox, 5.0, &|e| run(e, &[72, 76, 79, 84, 79, 76, 72, 67], 0.45, 0.7)),
        (Instrument::TubularBell, 6.0, &|e| run(e, &[60, 64, 55, 60], 1.4, 0.9)),
        (Instrument::ChurchBell, 7.0, &|e| run(e, &[48, 48, 48], 2.2, 1.0)),
        (Instrument::SingingBowl, 9.0, &|e| run(e, &[55], 0.0, 0.85)),
    ];

    for (inst, secs, phrase) in sections {
        let mut m = Mallet::new(FS, *inst, 24);
        let mut events: Vec<(f32, f32, f32)> = Vec::new(); // (time, midi, vel)
        phrase(&mut events);
        let n = (*secs * FS) as usize;
        let base = left.len();
        left.resize(base + n, 0.0);
        right.resize(base + n, 0.0);
        let mut ei = 0;
        events.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
        for i in 0..n {
            let t = i as f32 / FS;
            while ei < events.len() && events[ei].0 <= t {
                m.strike_midi(events[ei].1, events[ei].2);
                ei += 1;
            }
            let (l, r) = m.process();
            left[base + i] += l;
            right[base + i] += r;
        }
        t0 += *secs;
    }
    let _ = t0;

    let peak = left
        .iter()
        .chain(right.iter())
        .fold(0.0f32, |mx, &x| mx.max(x.abs()))
        .max(1e-9);
    write_wav("mallets_demo.wav", &left, &right, 0.89 / peak)?;
    println!("Wrote mallets_demo.wav: {:.1}s, peak {peak:.3}", left.len() as f32 / FS);
    Ok(())
}

fn run(events: &mut Vec<(f32, f32, f32)>, notes: &[i32], step: f32, vel: f32) {
    if step == 0.0 {
        events.push((0.3, notes[0] as f32, vel));
        return;
    }
    for (k, &n) in notes.iter().enumerate() {
        events.push((0.3 + k as f32 * step, n as f32, vel));
    }
}

fn chords(events: &mut Vec<(f32, f32, f32)>, chords: &[[i32; 3]], step: f32, vel: f32) {
    for (k, ch) in chords.iter().enumerate() {
        let t = 0.3 + k as f32 * step;
        for (j, &n) in ch.iter().enumerate() {
            events.push((t + j as f32 * 0.012, n as f32, vel));
        }
    }
}

fn write_wav(path: &str, left: &[f32], right: &[f32], gain: f32) -> std::io::Result<()> {
    let n = left.len();
    let (ch, bits, sr) = (2u16, 16u16, FS as u32);
    let ba = ch * bits / 8;
    let dl = (n as u32) * ba as u32;
    let mut w = BufWriter::new(File::create(path)?);
    w.write_all(b"RIFF")?;
    w.write_all(&(36 + dl).to_le_bytes())?;
    w.write_all(b"WAVE")?;
    w.write_all(b"fmt ")?;
    w.write_all(&16u32.to_le_bytes())?;
    w.write_all(&1u16.to_le_bytes())?;
    w.write_all(&ch.to_le_bytes())?;
    w.write_all(&sr.to_le_bytes())?;
    w.write_all(&(sr * ba as u32).to_le_bytes())?;
    w.write_all(&ba.to_le_bytes())?;
    w.write_all(&bits.to_le_bytes())?;
    w.write_all(b"data")?;
    w.write_all(&dl.to_le_bytes())?;
    for i in 0..n {
        for &s in &[left[i], right[i]] {
            w.write_all(&(((s * gain).clamp(-1.0, 1.0) * 32767.0) as i16).to_le_bytes())?;
        }
    }
    w.flush()
}
