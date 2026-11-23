//! Plantillas de proyecto por género y kits de batería (todo sintetizado, sin samples).
use crate::project::*;
use std::collections::HashMap;

pub const TEMPLATES: [&str; 8] = ["Vacío", "Trap", "Reggaetón (dembow)", "Boom bap", "House", "Lo-fi", "Drill", "Trap oscuro «Neblina»"];
pub const KITS: [&str; 6] = ["Boom bap polvoriento", "Trap 808", "Reggaetón", "House limpio", "Lo-fi suave", "Drill oscuro"];

fn steps(n: usize, on: &[usize], v: f32) -> Vec<f32> {
    let mut a = vec![0.0; n];
    for &i in on {
        if i < n {
            a[i] = v;
        }
    }
    a
}
fn note(s: u32, l: u32, m: u8, v: f32) -> Note {
    Note { s, l, m, v, o: 0.0 }
}
fn add(p: &mut Project, inst: Inst, name: &str, vol: f32) -> u32 {
    let id = p.new_id();
    let mut c = Channel::new(id, inst);
    c.name = name.into();
    c.vol = vol;
    p.channels.push(c);
    id
}
fn pattern(p: &mut Project, name: &str, bars: u32, ci: usize) -> Pattern {
    let id = p.new_id();
    Pattern { id, name: name.into(), bars, color: PALETTE[ci % PALETTE.len()], steps: HashMap::new(), notes: HashMap::new(), audio: HashMap::new() }
}
fn chord_notes(root: u8, ints: &[u8], s: u32, l: u32, v: f32) -> Vec<Note> {
    ints.iter().map(|i| note(s, l, root + i, v)).collect()
}

/// Crea un proyecto a partir de una plantilla.
pub fn build(i: usize) -> Project {
    match i {
        1 => trap(),
        2 => reggaeton(),
        3 => Project::demo(),
        4 => house(),
        5 => lofi(),
        6 => drill(),
        7 => neblina(),
        _ => {
            let mut p = Project::empty("Proyecto nuevo", 120.0);
            for (inst, n, v) in [(Inst::Kick, "Kick", 0.9), (Inst::Snare, "Snare", 0.8), (Inst::Hat, "Hat", 0.55), (Inst::Synth, "NN Wave", 0.7)] {
                add(&mut p, inst, n, v);
            }
            let pat = pattern(&mut p, "Patrón 1", 1, 0);
            p.patterns.push(pat);
            p
        }
    }
}

fn trap() -> Project {
    let mut p = Project::empty("Trap", 140.0);
    p.key = 1; // Do# menor
    p.scale = 1;
    let k = add(&mut p, Inst::Kick, "Kick", 0.85);
    let s = add(&mut p, Inst::Clap, "Clap", 0.8);
    let h = add(&mut p, Inst::Hat, "Hi-hat", 0.5);
    let o = add(&mut p, Inst::OHat, "Open Hat", 0.4);
    let b = add(&mut p, Inst::Bass, "808", 0.95);
    let l = add(&mut p, Inst::Synth, "Lead", 0.55);
    apply_kit(&mut p, 1);
    if let Some(c) = p.channel_mut(l) {
        c.synth = SynthP::preset(6);
        c.rev = 0.25;
    }
    if let Some(c) = p.channel_mut(b) {
        c.comp = CompP::ducking(k);
        c.comp.threshold = -26.0;
        c.comp.ratio = 4.0;
    }
    let mut d = pattern(&mut p, "Batería trap", 2, 0);
    d.steps.insert(k, steps(32, &[0, 10, 16, 22, 27], 0.95));
    d.steps.insert(s, steps(32, &[8, 24], 0.9));
    let mut hh: Vec<usize> = (0..32).step_by(2).collect();
    hh.extend([13, 14, 15, 29, 30, 31]);
    d.steps.insert(h, steps(32, &hh, 0.55));
    d.steps.insert(o, steps(32, &[22], 0.5));
    let mut m = pattern(&mut p, "808 y lead", 2, 1);
    m.notes.insert(b, vec![note(0, 6, 37, 0.95), note(10, 4, 37, 0.9), note(16, 6, 40, 0.95), note(22, 4, 35, 0.9), note(27, 5, 32, 0.9)]);
    m.notes.insert(l, vec![note(0, 3, 73, 0.8), note(3, 3, 76, 0.75), note(6, 2, 80, 0.8), note(8, 8, 78, 0.7), note(16, 3, 76, 0.8), note(19, 3, 75, 0.7), note(22, 10, 73, 0.75)]);
    let (di, mi) = (d.id, m.id);
    p.patterns = vec![d, m];
    for bar in (0..16).step_by(2) {
        p.playlist.push(Clip { track: 0, bar, pat: di });
        if bar >= 4 {
            p.playlist.push(Clip { track: 1, bar, pat: mi });
        }
    }
    p.tracks[0] = "Batería".into();
    p.tracks[1] = "808 / Lead".into();
    p
}

/// Pasos con velocidad propia: (paso, velocidad).
fn vsteps(n: usize, on: &[(usize, f32)]) -> Vec<f32> {
    let mut a = vec![0.0; n];
    for &(i, v) in on {
        if i < n {
            a[i] = v;
        }
    }
    a
}

/// Trap oscuro en La# menor frigio (la segunda menor, Si, le da el tono sombrío), 150 bpm a medio tiempo.
/// Estructura de 32 compases: intro (4) · coro (8) · verso (8) · coro (8) · salida (4).
fn neblina() -> Project {
    let mut p = Project::empty("Neblina", 150.0);
    p.key = 10; // La#
    p.scale = 4; // frigia
    p.swing = 0.06;
    p.limiter.gain_db = 7.0;
    let k = add(&mut p, Inst::Kick, "Kick", 0.9);
    let cl = add(&mut p, Inst::Clap, "Clap", 0.75);
    let sn = add(&mut p, Inst::Snare, "Snare capa", 0.45);
    let rim = add(&mut p, Inst::Rim, "Rim fantasma", 0.3);
    let h = add(&mut p, Inst::Hat, "Hi-hat", 0.42);
    let o = add(&mut p, Inst::OHat, "Open Hat", 0.3);
    let b = add(&mut p, Inst::Bass, "808", 0.95);
    let bell = add(&mut p, Inst::Bell, "Campana", 0.42);
    let pad = add(&mut p, Inst::Synth, "Pad niebla", 0.38);
    let ld = add(&mut p, Inst::Synth, "Lead glide", 0.34);
    apply_kit(&mut p, 5);
    if let Some(c) = p.channel_mut(k) {
        c.tune = -4.0;
    }
    if let Some(c) = p.channel_mut(sn) {
        c.tune = -2.0;
        c.rev = 0.15;
    }
    if let Some(c) = p.channel_mut(h) {
        c.pan = 0.18;
        c.hi = 1.0;
    }
    if let Some(c) = p.channel_mut(o) {
        c.pan = -0.2;
        c.rev = 0.2;
    }
    if let Some(c) = p.channel_mut(rim) {
        c.pan = -0.3;
        c.dly = 0.25;
    }
    if let Some(c) = p.channel_mut(b) {
        c.comp = CompP::ducking(k);
        c.comp.threshold = -24.0;
        c.comp.ratio = 4.0;
    }
    if let Some(c) = p.channel_mut(bell) {
        c.cut = 6000.0;
        c.rev = 0.35;
        c.dly = 0.3;
        c.pan = 0.12;
    }
    if let Some(c) = p.channel_mut(pad) {
        c.synth = SynthP::preset(11);
        c.rev = 0.45;
        c.space = 0.35;
        c.cut = 3500.0;
    }
    if let Some(c) = p.channel_mut(ld) {
        c.synth = SynthP::preset(18);
        c.rev = 0.3;
        c.dly = 0.35;
        c.pan = -0.12;
        c.cut = 4200.0;
    }

    // Batería del coro: bombo sincopado, clap en el 3, rim fantasma y hats con redobles
    let mut dc = pattern(&mut p, "Batería coro", 2, 0);
    dc.steps.insert(k, vsteps(32, &[(0, 0.98), (7, 0.8), (10, 0.9), (16, 0.98), (19, 0.75), (23, 0.7), (26, 0.9)]));
    dc.steps.insert(cl, steps(32, &[8, 24], 0.92));
    dc.steps.insert(sn, vsteps(32, &[(8, 0.8), (24, 0.8), (31, 0.4)]));
    dc.steps.insert(rim, vsteps(32, &[(5, 0.35), (14, 0.3), (21, 0.35), (29, 0.3)]));
    let mut hh: Vec<(usize, f32)> = Vec::new();
    for i in 0..32usize {
        let v = match i {
            12..=15 => 0.3 + 0.08 * (i - 12) as f32, // redoble que sube
            28..=31 => 0.62 - 0.08 * (i - 28) as f32, // redoble que baja
            _ if i % 4 == 0 => 0.62,
            _ if i % 2 == 0 => 0.45,
            _ if i % 8 == 3 || i % 8 == 7 => 0.0,
            _ => 0.28,
        };
        if v > 0.0 {
            hh.push((i, v));
        }
    }
    dc.steps.insert(h, vsteps(32, &hh));
    dc.steps.insert(o, steps(32, &[22], 0.45));

    // Batería del verso: más espacio, hats en corcheas
    let mut dv = pattern(&mut p, "Batería verso", 2, 0);
    dv.steps.insert(k, vsteps(32, &[(0, 0.95), (11, 0.8), (16, 0.95), (27, 0.75)]));
    dv.steps.insert(cl, steps(32, &[8, 24], 0.85));
    dv.steps.insert(rim, vsteps(32, &[(14, 0.3), (30, 0.3)]));
    let hv: Vec<(usize, f32)> = (0..32usize).step_by(2).map(|i| (i, if i % 4 == 0 { 0.55 } else { 0.38 })).chain([(29, 0.3), (30, 0.4), (31, 0.5)]).collect();
    dv.steps.insert(h, vsteps(32, &hv));

    // 808: sigue al bombo, sube al Si (segunda frigia) y cae al Fa#
    let mut m8 = pattern(&mut p, "808", 4, 1);
    m8.notes.insert(
        b,
        vec![
            note(0, 7, 34, 0.95),
            note(7, 3, 34, 0.8),
            note(10, 6, 46, 0.85),
            note(16, 3, 35, 0.95),
            note(19, 4, 35, 0.8),
            note(23, 3, 34, 0.75),
            note(26, 6, 34, 0.9),
            note(32, 7, 34, 0.95),
            note(39, 3, 34, 0.8),
            note(42, 6, 41, 0.85),
            note(48, 3, 30, 0.95),
            note(51, 4, 30, 0.8),
            note(55, 3, 29, 0.8),
            note(58, 6, 29, 0.9),
        ],
    );

    // Acordes en niebla: La#m · Si · La#m7 · Solb
    let mut pd = pattern(&mut p, "Pad niebla", 4, 2);
    let mut ac = chord_notes(58, &[0, 3, 7], 0, 16, 0.6);
    ac.extend(chord_notes(59, &[0, 4, 7], 16, 16, 0.55));
    ac.extend(chord_notes(58, &[0, 3, 7, 10], 32, 16, 0.6));
    ac.extend(chord_notes(54, &[0, 4, 7], 48, 16, 0.55));
    pd.notes.insert(pad, ac);

    // Gancho de campana (4 compases)
    let mut gb = pattern(&mut p, "Campana gancho", 4, 3);
    gb.notes.insert(
        bell,
        vec![
            note(0, 2, 82, 0.8),
            note(3, 2, 83, 0.7),
            note(6, 2, 82, 0.75),
            note(8, 6, 77, 0.8),
            note(14, 2, 78, 0.6),
            note(16, 2, 80, 0.75),
            note(19, 2, 78, 0.65),
            note(22, 2, 77, 0.7),
            note(24, 8, 73, 0.75),
            note(32, 2, 82, 0.8),
            note(35, 2, 83, 0.7),
            note(38, 2, 85, 0.8),
            note(40, 6, 83, 0.75),
            note(46, 2, 82, 0.6),
            note(48, 4, 78, 0.7),
            note(52, 4, 77, 0.65),
            note(56, 8, 70, 0.7),
        ],
    );

    // Lead con glide: respuesta larga que se arrastra entre notas (solo en el coro)
    let mut gl = pattern(&mut p, "Lead respuesta", 4, 4);
    gl.notes.insert(ld, vec![note(8, 6, 70, 0.7), note(14, 2, 71, 0.6), note(16, 12, 73, 0.7), note(40, 4, 77, 0.7), note(44, 4, 75, 0.65), note(48, 4, 73, 0.65), note(52, 10, 71, 0.6)]);

    let (dci, dvi, m8i, pdi, gbi, gli) = (dc.id, dv.id, m8.id, pd.id, gb.id, gl.id);
    p.patterns = vec![dc, dv, m8, pd, gb, gl];
    let clip = |p: &mut Project, track: u32, bar: u32, pat: u32| p.playlist.push(Clip { track, bar, pat });
    // intro 0-3: pad + campana
    clip(&mut p, 2, 0, pdi);
    clip(&mut p, 3, 0, gbi);
    // coros 4-11 y 20-27
    for start in [4u32, 20] {
        for bar in [start, start + 4] {
            clip(&mut p, 2, bar, pdi);
            clip(&mut p, 3, bar, gbi);
            clip(&mut p, 1, bar, m8i);
            clip(&mut p, 4, bar, gli);
        }
        for bar in (start..start + 8).step_by(2) {
            clip(&mut p, 0, bar, dci);
        }
    }
    // verso 12-19: batería suelta, 808 y pad; campana solo la segunda mitad
    for bar in [12, 16] {
        clip(&mut p, 1, bar, m8i);
        clip(&mut p, 2, bar, pdi);
    }
    clip(&mut p, 3, 16, gbi);
    for bar in (12..20).step_by(2) {
        clip(&mut p, 0, bar, dvi);
    }
    // salida 28-31: se quedan el pad y la campana
    clip(&mut p, 2, 28, pdi);
    clip(&mut p, 3, 28, gbi);
    for (i, n) in ["Batería", "808", "Pad", "Campana", "Lead"].iter().enumerate() {
        p.tracks[i] = (*n).into();
    }
    p
}

fn reggaeton() -> Project {
    let mut p = Project::empty("Reggaetón", 92.0);
    p.key = 9; // La menor
    p.scale = 1;
    let k = add(&mut p, Inst::Kick, "Kick", 0.9);
    let s = add(&mut p, Inst::Snare, "Snare dembow", 0.75);
    let r = add(&mut p, Inst::Rim, "Rim", 0.5);
    let h = add(&mut p, Inst::Hat, "Hat", 0.45);
    let b = add(&mut p, Inst::Bass, "Bajo", 0.85);
    let pd = add(&mut p, Inst::Pad, "Pad", 0.55);
    let pl = add(&mut p, Inst::Synth, "Pluck", 0.6);
    apply_kit(&mut p, 2);
    if let Some(c) = p.channel_mut(pl) {
        c.synth = SynthP::preset(1);
        c.dly = 0.25;
        c.rev = 0.2;
    }
    if let Some(c) = p.channel_mut(pd) {
        c.comp = CompP::ducking(k);
    }
    let mut d = pattern(&mut p, "Dembow", 1, 4);
    d.steps.insert(k, steps(16, &[0, 4, 8, 12], 0.95));
    d.steps.insert(s, steps(16, &[3, 6, 11, 14], 0.8));
    d.steps.insert(r, steps(16, &[7, 15], 0.45));
    d.steps.insert(h, steps(16, &[0, 2, 4, 6, 8, 10, 12, 14], 0.45));
    let mut m = pattern(&mut p, "Acordes Am–F–C–G", 4, 1);
    let prog = [(57u8, [0u8, 3, 7]), (53, [0, 4, 7]), (48, [0, 4, 7]), (55, [0, 4, 7])];
    let mut pads = vec![];
    let mut bass = vec![];
    let mut pluck = vec![];
    for (bar, (root, ints)) in prog.iter().enumerate() {
        let s0 = bar as u32 * 16;
        pads.extend(chord_notes(*root, ints, s0, 16, 0.6));
        for st in [0u32, 3, 6, 8, 11, 14] {
            bass.push(note(s0 + st, 2, root - 24, 0.85));
        }
        for (j, st) in [0u32, 3, 6, 10, 12].iter().enumerate() {
            pluck.push(note(s0 + st, 2, root + 12 + ints[j % 3], 0.7));
        }
    }
    m.notes.insert(pd, pads);
    m.notes.insert(b, bass);
    m.notes.insert(pl, pluck);
    let (di, mi) = (d.id, m.id);
    p.patterns = vec![d, m];
    for bar in 0..16 {
        p.playlist.push(Clip { track: 0, bar, pat: di });
    }
    for bar in (0..16).step_by(4) {
        p.playlist.push(Clip { track: 1, bar, pat: mi });
    }
    p.tracks[0] = "Dembow".into();
    p.tracks[1] = "Armonía".into();
    p
}

fn house() -> Project {
    let mut p = Project::empty("House", 124.0);
    p.key = 5; // Fa menor
    p.scale = 1;
    let k = add(&mut p, Inst::Kick, "Kick", 0.9);
    let c = add(&mut p, Inst::Clap, "Clap", 0.7);
    let o = add(&mut p, Inst::OHat, "Open Hat", 0.45);
    let h = add(&mut p, Inst::Hat, "Hat", 0.4);
    let b = add(&mut p, Inst::Synth, "Bajo", 0.75);
    let ch = add(&mut p, Inst::Rhodes, "Acordes", 0.6);
    apply_kit(&mut p, 3);
    if let Some(x) = p.channel_mut(b) {
        x.synth = SynthP::preset(3);
        x.comp = CompP::ducking(k);
    }
    if let Some(x) = p.channel_mut(ch) {
        x.comp = CompP::ducking(k);
        x.rev = 0.3;
        x.cut = 20000.0;
    }
    let mut d = pattern(&mut p, "Four on the floor", 1, 3);
    d.steps.insert(k, steps(16, &[0, 4, 8, 12], 0.95));
    d.steps.insert(c, steps(16, &[4, 12], 0.8));
    d.steps.insert(o, steps(16, &[2, 6, 10, 14], 0.5));
    d.steps.insert(h, steps(16, &[1, 3, 5, 7, 9, 11, 13, 15], 0.3));
    let mut m = pattern(&mut p, "Bajo y acordes", 2, 2);
    m.notes.insert(b, vec![note(2, 2, 29, 0.9), note(6, 2, 29, 0.9), note(10, 2, 32, 0.9), note(14, 2, 29, 0.9), note(18, 2, 27, 0.9), note(22, 2, 27, 0.9), note(26, 2, 32, 0.9), note(30, 2, 34, 0.9)]);
    let mut cn = chord_notes(53, &[0, 3, 7, 10], 2, 2, 0.7);
    cn.extend(chord_notes(53, &[0, 3, 7, 10], 10, 2, 0.7));
    cn.extend(chord_notes(51, &[0, 4, 7, 11], 18, 2, 0.7));
    cn.extend(chord_notes(51, &[0, 4, 7, 11], 26, 2, 0.7));
    m.notes.insert(ch, cn);
    let (di, mi) = (d.id, m.id);
    p.patterns = vec![d, m];
    for bar in 0..16 {
        p.playlist.push(Clip { track: 0, bar, pat: di });
    }
    for bar in (4..16).step_by(2) {
        p.playlist.push(Clip { track: 1, bar, pat: mi });
    }
    // automatización de ejemplo: el filtro de los acordes se abre
    let aid = p.new_id();
    p.autos.push(AutoClip { id: aid, track: 2, start: 4 * 16, len: 4 * 16, target: AutoTarget::Cut(ch), pts: vec![[0.0, 0.35], [1.0, 1.0]] });
    p.tracks[0] = "Batería".into();
    p.tracks[1] = "Bajo / acordes".into();
    p.tracks[2] = "Automatización".into();
    p
}

fn lofi() -> Project {
    let mut p = Project::empty("Lo-fi", 78.0);
    p.key = 2; // Re menor
    p.scale = 3; // dórica
    p.swing = 0.32;
    p.vinyl = 0.7;
    let k = add(&mut p, Inst::Kick, "Kick", 0.85);
    let s = add(&mut p, Inst::Snare, "Snare", 0.7);
    let h = add(&mut p, Inst::Hat, "Hat", 0.45);
    let r = add(&mut p, Inst::Rhodes, "Rhodes", 0.75);
    let b = add(&mut p, Inst::Bass, "Bajo", 0.75);
    apply_kit(&mut p, 4);
    if let Some(x) = p.channel_mut(r) {
        x.rev = 0.3;
        x.cut = 2400.0;
        x.strum = 0.025;
    }
    let mut d = pattern(&mut p, "Batería lo-fi", 2, 5);
    d.steps.insert(k, steps(32, &[0, 7, 10, 16, 26], 0.9));
    d.steps.insert(s, steps(32, &[8, 24], 0.75));
    d.steps.insert(h, steps(32, &(0..32).step_by(2).collect::<Vec<_>>(), 0.45));
    let mut m = pattern(&mut p, "Acordes Dm9–G13", 2, 6);
    let mut rn = chord_notes(50, &[0, 3, 10, 14, 17], 0, 16, 0.7);
    rn.extend(chord_notes(43, &[0, 10, 16, 21], 16, 16, 0.7));
    m.notes.insert(r, rn);
    m.notes.insert(b, vec![note(0, 6, 38, 0.8), note(10, 4, 38, 0.6), note(16, 6, 31, 0.8), note(26, 4, 33, 0.6)]);
    let (di, mi) = (d.id, m.id);
    p.patterns = vec![d, m];
    for bar in (0..16).step_by(2) {
        p.playlist.push(Clip { track: 0, bar, pat: di });
        p.playlist.push(Clip { track: 1, bar, pat: mi });
    }
    p.tracks[0] = "Batería".into();
    p.tracks[1] = "Rhodes".into();
    p
}

fn drill() -> Project {
    let mut p = Project::empty("Drill", 142.0);
    p.key = 6; // Fa# menor
    p.scale = 2; // menor armónica
    let k = add(&mut p, Inst::Kick, "Kick", 0.85);
    let s = add(&mut p, Inst::Snare, "Snare", 0.8);
    let h = add(&mut p, Inst::Hat, "Hat", 0.5);
    let b = add(&mut p, Inst::Bass, "808 slide", 0.95);
    let pd = add(&mut p, Inst::Synth, "Cuerdas", 0.5);
    apply_kit(&mut p, 5);
    if let Some(x) = p.channel_mut(pd) {
        x.synth = SynthP::preset(2);
        x.rev = 0.35;
    }
    let mut d = pattern(&mut p, "Batería drill", 2, 4);
    d.steps.insert(k, steps(32, &[0, 11, 22], 0.95));
    d.steps.insert(s, steps(32, &[12, 28], 0.85));
    d.steps.insert(h, steps(32, &[0, 3, 6, 8, 11, 14, 16, 19, 22, 24, 26, 27, 30], 0.5));
    let mut m = pattern(&mut p, "808 y cuerdas", 2, 2);
    m.notes.insert(b, vec![note(0, 8, 30, 0.95), note(11, 5, 33, 0.9), note(22, 8, 29, 0.95)]);
    let mut pn = chord_notes(54, &[0, 3, 7], 0, 16, 0.6);
    pn.extend(chord_notes(50, &[0, 4, 7], 16, 8, 0.6));
    pn.extend(chord_notes(49, &[0, 4, 7], 24, 8, 0.6));
    m.notes.insert(pd, pn);
    let (di, mi) = (d.id, m.id);
    p.patterns = vec![d, m];
    for bar in (0..16).step_by(2) {
        p.playlist.push(Clip { track: 0, bar, pat: di });
        p.playlist.push(Clip { track: 1, bar, pat: mi });
    }
    p.tracks[0] = "Batería".into();
    p.tracks[1] = "808 / cuerdas".into();
    p
}

/// Aplica un kit de batería (afinación, caída, EQ y compresión) a los canales de percusión.
pub fn apply_kit(p: &mut Project, kit: usize) {
    for c in p.channels.iter_mut().filter(|c| c.inst.is_drum()) {
        let (tune, decay, lo, hi, comp) = match (kit, c.inst) {
            (0, Inst::Kick) => (0.0, 1.0, 2.0, -2.0, true),
            (0, Inst::Snare) => (-1.0, 1.1, 0.0, -1.0, true),
            (0, _) => (0.0, 1.0, 0.0, -3.0, false),
            (1, Inst::Kick) => (-2.0, 0.8, 3.0, 1.0, true),
            (1, Inst::Clap) | (1, Inst::Snare) => (1.0, 0.9, -2.0, 2.0, true),
            (1, Inst::Hat) => (3.0, 0.6, 0.0, 3.0, false),
            (1, _) => (2.0, 0.8, 0.0, 2.0, false),
            (2, Inst::Kick) => (0.0, 0.7, 2.0, 2.0, true),
            (2, Inst::Snare) => (3.0, 0.6, -3.0, 3.0, true),
            (2, _) => (2.0, 0.7, 0.0, 2.0, false),
            (3, Inst::Kick) => (-1.0, 0.9, 3.0, 0.0, true),
            (3, Inst::Clap) => (0.0, 1.1, -2.0, 2.0, false),
            (3, _) => (1.0, 1.0, -2.0, 2.0, false),
            (4, Inst::Kick) => (-1.0, 1.2, 1.0, -5.0, false),
            (4, _) => (-2.0, 1.1, -1.0, -6.0, false),
            (5, Inst::Kick) => (-3.0, 0.7, 2.0, -1.0, true),
            (5, Inst::Snare) => (2.0, 0.8, -3.0, 3.0, true),
            (5, _) => (4.0, 0.5, 0.0, 2.0, false),
            _ => (0.0, 1.0, 0.0, 0.0, false),
        };
        c.tune = tune;
        c.decay = decay;
        c.lo = lo;
        c.hi = hi;
        c.comp = CompP::channel();
        if comp {
            c.comp.on = true;
            c.comp.threshold = -16.0;
            c.comp.ratio = 4.0;
            c.comp.attack_ms = 8.0;
            c.comp.release_ms = 90.0;
            c.comp.makeup = 2.0;
        }
    }
}

/// Progresiones de acordes (grados de la escala, base 0) para el ayudante del piano roll.
pub const PROGRESSIONS: [(&str, [u8; 4]); 8] = [
    ("i – VI – III – VII (épica)", [0, 5, 2, 6]),
    ("i – iv – VII – III (trap)", [0, 3, 6, 2]),
    ("I – V – vi – IV (pop)", [0, 4, 5, 3]),
    ("ii – V – I – I (jazz)", [1, 4, 0, 0]),
    ("vi – IV – I – V (balada)", [5, 3, 0, 4]),
    ("i – VII – VI – VII (reggaetón)", [0, 6, 5, 6]),
    ("I – IV – I – V (clásica)", [0, 3, 0, 4]),
    ("i – v – iv – i (oscura)", [0, 4, 3, 0]),
];

/// Construye un acorde (tríada o séptima) sobre el grado `deg` de la escala del proyecto.
pub fn diatonic_chord(p: &Project, deg: u8, octave_root: i32, seventh: bool) -> Vec<u8> {
    let scale: Vec<i32> = (0..12).filter(|&i| SCALES[(p.scale as usize).min(SCALES.len() - 1)].1[i as usize]).collect();
    if scale.len() < 5 {
        let r = octave_root + p.key as i32;
        return vec![r as u8, (r + 4) as u8, (r + 7) as u8];
    }
    let n = scale.len() as i32;
    let pick = |k: i32| -> i32 {
        let o = k.div_euclid(n);
        let i = k.rem_euclid(n);
        octave_root + p.key as i32 + scale[i as usize] + 12 * o
    };
    let d = deg as i32;
    let mut v = vec![pick(d), pick(d + 2), pick(d + 4)];
    if seventh {
        v.push(pick(d + 6));
    }
    v.into_iter().filter(|m| (0..=127).contains(m)).map(|m| m as u8).collect()
}
