//! "Shur Journey" — a multi-instrument instrumental in Dastgah-e Shur, built
//! entirely from the Puget physical-modeling voice library: a barbat drone, a
//! tombak/daf/dohol percussion bed, and layered tar / kamancheh / ney / santur
//! melodies, mixed through the shared room reverb. Renders a stereo WAV.
//!
//! `cargo run -p songs --bin shur_journey --release`
//!
//! Proprietary. Copyright (c) 2026 Avery Wagar. All rights reserved.

use std::fs::File;
use std::io::{BufWriter, Write};

use bowed_core::{Bowed, StringKind};
use drum_core::{Daf, Dohol, Tombak};
use plucked_core::{Barbat, Santur, Tar};
use puget_dsp::Reverb;
use wind_core::{Wind, WindKind};

const FS: f32 = 48_000.0;
const BEAT: f32 = 0.55; // seconds per beat (~109 BPM)
const BAR: f32 = 4.0 * BEAT; // 4/4

// Dastgah-e Shur degrees (cents within the octave); index by scale degree.
const SHUR: [f32; 7] = [0.0, 149.0, 300.0, 500.0, 702.0, 783.0, 985.0];

/// Hz of a Shur scale degree above/below a root (degree can be negative or > 6;
/// it wraps by octaves).
fn shur(root: f32, deg: i32) -> f32 {
    let oct = deg.div_euclid(7);
    let idx = deg.rem_euclid(7) as usize;
    let cents = SHUR[idx] + oct as f32 * 1200.0;
    root * 2f32.powf(cents / 1200.0)
}

fn s(sec: f32) -> usize {
    (sec * FS) as usize
}
/// Sample index at bar `b` (0-based) + beat `beat` within it.
fn at(b: f32, beat: f32) -> usize {
    s(b * BAR + beat * BEAT)
}

const ROOT: f32 = 293.66; // D4, the melodic tonic (shahed)
const TOTAL_BARS: f32 = 44.0;

fn main() -> std::io::Result<()> {
    let total = s(TOTAL_BARS * BAR + 4.0); // + a 4 s tail
    let mut l = vec![0.0f32; total];
    let mut r = vec![0.0f32; total];

    barbat_drone(&mut l, &mut r);
    santur_part(&mut l, &mut r);
    tar_part(&mut l, &mut r);
    kamancheh_part(&mut l, &mut r);
    ney_part(&mut l, &mut r);
    tombak_part(&mut l, &mut r);
    daf_part(&mut l, &mut r);
    dohol_part(&mut l, &mut r);

    // Room reverb over the whole mix.
    let mut rv = Reverb::new(FS);
    rv.set_size(0.62);
    rv.set_mix(0.20);
    for i in 0..total {
        let (wl, wr) = rv.process(l[i], r[i]);
        l[i] = wl;
        r[i] = wr;
    }

    // A short fade-in and a resolving fade-out over the final phrase.
    let fade_in = s(0.4);
    let fade_out = s(6.0);
    for i in 0..total {
        let mut g = 1.0;
        if i < fade_in {
            g *= i as f32 / fade_in as f32;
        }
        if i >= total - fade_out {
            g *= (total - i) as f32 / fade_out as f32;
        }
        l[i] *= g;
        r[i] *= g;
    }

    // Normalize with a little headroom.
    let peak = l.iter().chain(r.iter()).fold(0.0f32, |m, &x| m.max(x.abs())).max(1e-9);
    write_wav("shur_journey.wav", &l, &r, 0.89 / peak)?;
    println!("Wrote shur_journey.wav: {:.1}s", total as f32 / FS);
    Ok(())
}

// ── Foundation: barbat drone (whole piece) ──────────────────────────────────
fn barbat_drone(l: &mut [f32], r: &mut [f32]) {
    let mut b = Barbat::new(FS);
    b.set_brightness(0.35);
    b.set_sustain(0.9);
    // Re-pluck the tonic (D2/D3) every two bars so the drone never dies.
    let mut trig: Vec<(usize, f32, f32)> = Vec::new();
    let mut bar = 0.0;
    while bar < TOTAL_BARS {
        trig.push((at(bar, 0.0), shur(ROOT * 0.25, 0), 0.55)); // D2
        trig.push((at(bar, 0.05), shur(ROOT * 0.5, 0), 0.42)); // D3
        bar += 2.0;
    }
    run_stereo(l, r, &mut b, &trig, |b, hz, v| b.pluck(hz, v), |b| b.process(), 0.5);
}

// ── Santur: intro shimmer + fills ───────────────────────────────────────────
fn santur_part(l: &mut [f32], r: &mut [f32]) {
    let mut sn = Santur::new(FS);
    sn.set_brightness(0.55);
    sn.set_sustain(0.85);
    sn.set_shimmer(6.0);
    let mut trig: Vec<(usize, f32, f32)> = Vec::new();
    // Intro (bars 0-8): slow ascending/descending runs.
    let run = [0, 2, 3, 4, 5, 4, 3, 2];
    for bar in (0..8).step_by(2) {
        for (k, &d) in run.iter().enumerate() {
            trig.push((at(bar as f32, k as f32 * 0.25), shur(ROOT, d + 7), 0.5));
        }
    }
    // Fills through the B section (bars 20-32): quick tremolo flourishes.
    for bar in (21..32).step_by(4) {
        for k in 0..8 {
            let d = [7, 9, 11, 12, 11, 9, 7, 9][k];
            trig.push((at(bar as f32, 2.0 + k as f32 * 0.18), shur(ROOT, d - 7), 0.45));
        }
    }
    // Climax cascade (bars 32-40).
    for bar in (32..40).step_by(2) {
        for k in 0..12 {
            trig.push((at(bar as f32, k as f32 * 0.16), shur(ROOT, (k as i32 % 8) + 3), 0.4));
        }
    }
    run_stereo(l, r, &mut sn, &trig, |sn, hz, v| sn.strike(hz, v), |sn| sn.process(), 0.5);
}

// ── Tar: theme A (bars 8-20) + climax counterpoint ──────────────────────────
fn tar_part(l: &mut [f32], r: &mut [f32]) {
    let mut t = Tar::new(FS);
    t.set_brightness(0.55);
    t.set_sustain(0.75);
    // Theme A as (degree, beats).
    let theme: &[(i32, f32)] = &[
        (0, 1.0), (2, 1.0), (3, 0.5), (4, 0.5), (3, 1.0),
        (2, 1.0), (0, 0.5), (-1, 0.5), (0, 2.0),
        (4, 1.0), (5, 1.0), (4, 0.5), (3, 0.5), (2, 1.0),
        (3, 1.0), (2, 0.5), (0, 0.5), (0, 2.0),
    ];
    let mut trig: Vec<(usize, f32, f32)> = Vec::new();
    // State the theme across bars 8-20 (two passes).
    for &pass_bar in &[8.0f32, 14.0] {
        let mut t_beats = 0.0;
        for &(d, dur) in theme {
            trig.push((at(pass_bar, t_beats), shur(ROOT, d), 0.85));
            t_beats += dur;
        }
    }
    // Climax: sparse high answers (bars 32-40).
    let ans = [7, 5, 4, 5, 7, 9];
    for (k, &d) in ans.iter().enumerate() {
        trig.push((at(33.0 + k as f32 * 1.2, 0.0), shur(ROOT, d), 0.8));
    }
    run_stereo(l, r, &mut t, &trig, |t, hz, v| t.pluck(hz, v), |t| t.process(), 0.55);
}

// ── Kamancheh: lead melody (bars 4 intro, 20-32 B, 32-44 climax/outro) ───────
fn kamancheh_part(l: &mut [f32], r: &mut [f32]) {
    let mut k = Bowed::new(FS, StringKind::Kamancheh);
    k.set_brightness(0.5);
    // (degree, beats) phrases; note_on then note_off just before next.
    let intro: &[(i32, f32)] = &[(4, 3.0), (3, 2.0), (5, 3.0), (4, 4.0), (2, 4.0)];
    let lead: &[(i32, f32)] = &[
        (4, 2.0), (5, 2.0), (7, 3.0), (5, 1.0), (4, 2.0), (3, 2.0), (4, 4.0),
        (7, 2.0), (8, 2.0), (9, 3.0), (8, 1.0), (7, 2.0), (5, 2.0), (4, 4.0),
    ];
    let outro: &[(i32, f32)] = &[(4, 4.0), (3, 3.0), (2, 3.0), (0, 6.0)];

    let mut events: Vec<(usize, Option<(f32, f32)>)> = Vec::new();
    let push_phrase = |start_bar: f32, phrase: &[(i32, f32)], vib: f32, events: &mut Vec<_>| {
        let mut tb = 0.0;
        for &(d, dur) in phrase {
            let on = at(start_bar, tb);
            let off = at(start_bar, tb + dur * 0.92);
            events.push((on, Some((shur(ROOT, d), 0.7))));
            events.push((off, None));
            tb += dur;
        }
        let _ = vib;
    };
    push_phrase(4.0, intro, 0.03, &mut events);
    push_phrase(20.0, lead, 0.035, &mut events);
    push_phrase(32.0, lead, 0.04, &mut events); // reprise, climax
    push_phrase(40.0, outro, 0.03, &mut events);
    events.sort_by_key(|e| e.0);

    k.set_vibrato(5.5, 0.035);
    run_bowed(l, r, &mut k, &events, 0.6, 0.62, 0.38); // panned slightly right
}

// ── Ney: a soaring line in the climax (bars 32-40) ──────────────────────────
fn ney_part(l: &mut [f32], r: &mut [f32]) {
    let mut n = Wind::new(FS, WindKind::Ney);
    n.set_brightness(0.5);
    n.set_vibrato(5.0, 0.03);
    // Stays at/above D4 (ney is happiest C4 up).
    let line: &[(i32, f32)] = &[(7, 3.0), (5, 2.0), (4, 3.0), (7, 2.0), (9, 4.0), (7, 2.0), (5, 4.0)];
    let mut events: Vec<(usize, Option<(f32, f32)>)> = Vec::new();
    let mut tb = 0.0;
    for &(d, dur) in line {
        events.push((at(33.0, tb), Some((shur(ROOT, d), 0.85))));
        events.push((at(33.0, tb + dur * 0.9), None));
        tb += dur;
    }
    events.sort_by_key(|e| e.0);
    run_ney(l, r, &mut n, &events, 0.32, 0.4, 0.6); // panned slightly left
}

// ── Tombak groove (bars 8-40) ───────────────────────────────────────────────
fn tombak_part(l: &mut [f32], r: &mut [f32]) {
    let mut d = Tombak::new(FS);
    d.set_tune(104.0);
    // 16-step grid: T=tom, k=bak, f=finger, .=rest
    let pat = b"T..kf.k.T.k.f.k.";
    let mut trig: Vec<(usize, u8, f32)> = Vec::new();
    for bar in 8..40 {
        for (i, &c) in pat.iter().enumerate() {
            if c == b'.' {
                continue;
            }
            let vel = if i == 0 { 1.0 } else if c == b'f' { 0.5 } else { 0.75 };
            trig.push((at(bar as f32, i as f32 * 0.25), c, vel));
        }
    }
    run_drum3(l, r, &mut d, &trig, 0.6);
}

// ── Daf (bars 20-40): frame drum + a shake at each section top ───────────────
fn daf_part(l: &mut [f32], r: &mut [f32]) {
    let mut d = Daf::new(FS);
    d.set_jingle(0.7);
    let pat = b"D..t..t.D..t.t..";
    let mut ev: Vec<(usize, u8, f32)> = Vec::new(); // 'D' dum, 't' tak, 's' shake
    for bar in 20..40 {
        for (i, &c) in pat.iter().enumerate() {
            if c == b'.' {
                continue;
            }
            ev.push((at(bar as f32, i as f32 * 0.25), c, if c == b'D' { 0.8 } else { 0.55 }));
        }
    }
    ev.push((at(20.0, 0.0), b's', 0.7));
    ev.push((at(32.0, 0.0), b's', 0.9));
    run_daf(l, r, &mut d, &ev, 0.5);
}

// ── Dohol: deep accents (downbeats in B; climax hits) ────────────────────────
fn dohol_part(l: &mut [f32], r: &mut [f32]) {
    let mut d = Dohol::new(FS);
    d.set_tune(66.0);
    d.set_decay(0.6);
    let mut trig: Vec<(usize, f32, f32)> = Vec::new(); // (sample, vel, pos)
    for bar in 20..32 {
        trig.push((at(bar as f32, 0.0), 0.9, 0.12)); // boom on 1
    }
    for bar in 32..40 {
        trig.push((at(bar as f32, 0.0), 1.0, 0.12));
        trig.push((at(bar as f32, 2.0), 0.8, 0.12));
        trig.push((at(bar as f32, 3.5), 0.6, 0.9)); // rim pickup
    }
    run_stereo(l, r, &mut d, &trig, |d, pos, v| d.strike(v, pos), |d| d.process(), 0.7);
}

// ── Generic render loops ────────────────────────────────────────────────────

/// A stereo-output voice with a (sample, arg, vel) trigger (`strike(arg, vel)`).
fn run_stereo<V>(
    l: &mut [f32],
    r: &mut [f32],
    v: &mut V,
    trig: &[(usize, f32, f32)],
    mut fire: impl FnMut(&mut V, f32, f32),
    mut proc: impl FnMut(&mut V) -> (f32, f32),
    gain: f32,
) {
    let mut sorted = trig.to_vec();
    sorted.sort_by_key(|t| t.0);
    let mut ti = 0;
    for i in 0..l.len() {
        while ti < sorted.len() && sorted[ti].0 == i {
            fire(v, sorted[ti].1, sorted[ti].2);
            ti += 1;
        }
        let (a, b) = proc(v);
        l[i] += a * gain;
        r[i] += b * gain;
    }
}

/// A bowed voice: events are note-on `Some((hz,vel))` / note-off `None`.
fn run_bowed(
    l: &mut [f32],
    r: &mut [f32],
    v: &mut Bowed,
    events: &[(usize, Option<(f32, f32)>)],
    gain: f32,
    pan_l: f32,
    pan_r: f32,
) {
    let mut ti = 0;
    for i in 0..l.len() {
        while ti < events.len() && events[ti].0 == i {
            match events[ti].1 {
                Some((hz, vel)) => v.note_on(hz, vel, 0.5),
                None => v.note_off(),
            }
            ti += 1;
        }
        let x = v.process() * gain;
        l[i] += x * pan_l;
        r[i] += x * pan_r;
    }
}

/// The ney (wind): note-on `Some((hz,breath))` / note-off `None`.
fn run_ney(
    l: &mut [f32],
    r: &mut [f32],
    v: &mut Wind,
    events: &[(usize, Option<(f32, f32)>)],
    gain: f32,
    pan_l: f32,
    pan_r: f32,
) {
    let mut ti = 0;
    for i in 0..l.len() {
        while ti < events.len() && events[ti].0 == i {
            match events[ti].1 {
                Some((hz, breath)) => v.note_on(hz, breath),
                None => v.note_off(),
            }
            ti += 1;
        }
        let x = v.process() * gain;
        l[i] += x * pan_l;
        r[i] += x * pan_r;
    }
}

/// The tombak: (sample, stroke byte, vel), stroke in {T,k,f}.
fn run_drum3(l: &mut [f32], r: &mut [f32], v: &mut Tombak, trig: &[(usize, u8, f32)], gain: f32) {
    let mut sorted = trig.to_vec();
    sorted.sort_by_key(|t| t.0);
    let mut ti = 0;
    for i in 0..l.len() {
        while ti < sorted.len() && sorted[ti].0 == i {
            match sorted[ti].1 {
                b'T' => v.tom(sorted[ti].2),
                b'k' => v.bak(sorted[ti].2),
                _ => v.finger(sorted[ti].2),
            }
            ti += 1;
        }
        let (a, b) = v.process();
        l[i] += a * gain;
        r[i] += b * gain;
    }
}

/// The daf: (sample, stroke byte, vel), stroke in {D,t,s(hake)}.
fn run_daf(l: &mut [f32], r: &mut [f32], v: &mut Daf, trig: &[(usize, u8, f32)], gain: f32) {
    let mut sorted = trig.to_vec();
    sorted.sort_by_key(|t| t.0);
    let mut ti = 0;
    for i in 0..l.len() {
        while ti < sorted.len() && sorted[ti].0 == i {
            match sorted[ti].1 {
                b'D' => v.dum(sorted[ti].2),
                b't' => v.tak(sorted[ti].2),
                _ => v.shake(sorted[ti].2),
            }
            ti += 1;
        }
        let (a, b) = v.process();
        l[i] += a * gain;
        r[i] += b * gain;
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
        for &x in &[left[i], right[i]] {
            w.write_all(&(((x * gain).clamp(-1.0, 1.0) * 32767.0) as i16).to_le_bytes())?;
        }
    }
    w.flush()
}
