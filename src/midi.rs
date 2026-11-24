//! MIDI: lectura y escritura de archivos .mid (SMF 0/1) y entrada de teclados MIDI.
use crate::project::{Inst, Note, Pattern, Project, STEPS_PER_BAR};

/// Nota leída de un archivo: inicio y duración en pasos (semicorcheas) con fracción.
#[derive(Clone, Copy, Debug)]
pub struct MNote {
    pub start: f64,
    pub len: f64,
    pub key: u8,
    pub vel: f32,
    pub chan: u8,
}

pub struct MTrack {
    pub name: String,
    pub notes: Vec<MNote>,
}

pub struct MidiFile {
    pub tracks: Vec<MTrack>,
    pub bpm: Option<f32>,
}

fn vlq(b: &[u8], i: &mut usize) -> Option<u32> {
    let mut v = 0u32;
    for _ in 0..4 {
        let x = *b.get(*i)?;
        *i += 1;
        v = (v << 7) | (x & 0x7f) as u32;
        if x & 0x80 == 0 {
            return Some(v);
        }
    }
    Some(v)
}

/// Lee un archivo MIDI estándar.
pub fn read(bytes: &[u8]) -> Result<MidiFile, String> {
    if bytes.len() < 14 || &bytes[0..4] != b"MThd" {
        return Err("No es un archivo MIDI.".into());
    }
    let ntr = u16::from_be_bytes([bytes[10], bytes[11]]) as usize;
    let div = u16::from_be_bytes([bytes[12], bytes[13]]);
    if div & 0x8000 != 0 {
        return Err("MIDI con tiempo SMPTE no soportado.".into());
    }
    let tpq = div.max(1) as f64;
    let mut i = 8 + u32::from_be_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]) as usize;
    let mut tracks = vec![];
    let mut bpm = None;
    for _ in 0..ntr {
        if i + 8 > bytes.len() || &bytes[i..i + 4] != b"MTrk" {
            break;
        }
        let len = u32::from_be_bytes([bytes[i + 4], bytes[i + 5], bytes[i + 6], bytes[i + 7]]) as usize;
        let body = &bytes[i + 8..(i + 8 + len).min(bytes.len())];
        i += 8 + len;
        let mut p = 0usize;
        let mut t = 0u64;
        let mut status = 0u8;
        let mut name = String::new();
        let mut open: Vec<(u8, u8, u64, f32)> = vec![];
        let mut notes = vec![];
        while p < body.len() {
            let Some(d) = vlq(body, &mut p) else { break };
            t += d as u64;
            let Some(&b0) = body.get(p) else { break };
            if b0 == 0xff {
                let ty = *body.get(p + 1).unwrap_or(&0);
                p += 2;
                let l = vlq(body, &mut p).unwrap_or(0) as usize;
                let data = &body[p.min(body.len())..(p + l).min(body.len())];
                if ty == 0x03 && name.is_empty() {
                    name = String::from_utf8_lossy(data).trim().to_string();
                }
                if ty == 0x51 && data.len() == 3 && bpm.is_none() {
                    let us = ((data[0] as u32) << 16) | ((data[1] as u32) << 8) | data[2] as u32;
                    if us > 0 {
                        bpm = Some((60_000_000.0 / us as f64) as f32);
                    }
                }
                p += l;
                continue;
            }
            if b0 == 0xf0 || b0 == 0xf7 {
                p += 1;
                let l = vlq(body, &mut p).unwrap_or(0) as usize;
                p += l;
                continue;
            }
            if b0 & 0x80 != 0 {
                status = b0;
                p += 1;
            }
            let kind = status & 0xf0;
            let ch = status & 0x0f;
            let n = if kind == 0xc0 || kind == 0xd0 { 1 } else { 2 };
            let a = *body.get(p).unwrap_or(&0);
            let b = if n == 2 { *body.get(p + 1).unwrap_or(&0) } else { 0 };
            p += n;
            let mut off = |key: u8, ch: u8, t: u64, notes: &mut Vec<MNote>| {
                if let Some(k) = open.iter().position(|o| o.0 == key && o.1 == ch) {
                    let o = open.remove(k);
                    let st = o.2 as f64 / tpq * 4.0;
                    let ln = (t.saturating_sub(o.2)) as f64 / tpq * 4.0;
                    notes.push(MNote { start: st, len: ln.max(0.25), key, vel: o.3, chan: ch });
                }
            };
            match kind {
                0x90 if b > 0 => {
                    off(a, ch, t, &mut notes);
                    open.push((a, ch, t, b as f32 / 127.0));
                }
                0x80 | 0x90 => off(a, ch, t, &mut notes),
                _ => {}
            }
        }
        for o in open {
            notes.push(MNote { start: o.2 as f64 / tpq * 4.0, len: 1.0, key: o.0, vel: o.3, chan: o.1 });
        }
        if !notes.is_empty() {
            tracks.push(MTrack { name, notes });
        }
    }
    Ok(MidiFile { tracks, bpm })
}

/// Percusión General MIDI → instrumento de batería.
pub fn gm_drum(key: u8) -> Option<Inst> {
    Some(match key {
        35 | 36 => Inst::Kick,
        38 | 40 => Inst::Snare,
        39 => Inst::Clap,
        37 => Inst::Rim,
        75 | 76 | 77 => Inst::Clave,
        42 | 44 => Inst::Hat,
        54 | 69 | 70 | 82 => Inst::Shaker,
        46 => Inst::OHat,
        49 | 52 | 55 | 57 => Inst::Crash,
        51 | 53 | 59 => Inst::Ride,
        56 => Inst::Cowbell,
        41 | 43 | 45 | 47 | 48 | 50 => Inst::Tom,
        60..=64 => Inst::Conga,
        _ => return None,
    })
}
pub fn drum_gm(i: Inst) -> u8 {
    match i {
        Inst::Kick => 36,
        Inst::Snare => 38,
        Inst::Clap => 39,
        Inst::Rim => 37,
        Inst::Hat => 42,
        Inst::OHat => 46,
        Inst::Tom => 45,
        Inst::Conga => 63,
        Inst::Cowbell => 56,
        Inst::Clave => 75,
        Inst::Snap => 39,
        Inst::Shaker => 82,
        Inst::Ride => 51,
        Inst::Crash => 49,
        _ => 60,
    }
}

/// Importa un archivo MIDI como un patrón nuevo: una pista por canal melódico, percusión GM a la batería.
pub fn import(p: &mut Project, mf: &MidiFile, name: &str, default_inst: Inst) -> Result<u32, String> {
    if mf.tracks.is_empty() {
        return Err("El archivo MIDI no tiene notas.".into());
    }
    let last = mf.tracks.iter().flat_map(|t| t.notes.iter()).map(|n| n.start + n.len).fold(0.0, f64::max);
    let bars = ((last / STEPS_PER_BAR as f64).ceil() as u32).clamp(1, 64);
    let pid = p.new_id();
    let color = crate::project::PALETTE[p.patterns.len() % crate::project::PALETTE.len()];
    let mut pat = Pattern { id: pid, name: name.chars().take(40).collect(), bars, color, steps: Default::default(), notes: Default::default(), audio: Default::default() };
    let n = pat.len_steps() as usize;
    for (ti, t) in mf.tracks.iter().enumerate() {
        let (drums, mel): (Vec<MNote>, Vec<MNote>) = t.notes.iter().partition(|x| x.chan == 9);
        for d in drums {
            let Some(inst) = gm_drum(d.key) else { continue };
            let id = match p.channels.iter().find(|c| c.inst == inst) {
                Some(c) => c.id,
                None => {
                    if p.channels.len() >= crate::engine::MAX_CH {
                        continue;
                    }
                    let id = p.new_id();
                    p.channels.push(crate::project::Channel::new(id, inst));
                    id
                }
            };
            let s = d.start.round() as usize;
            if s < n {
                let a = pat.steps.entry(id).or_insert_with(|| vec![0.0; n]);
                a[s] = a[s].max(d.vel.max(0.1));
            }
        }
        if mel.is_empty() || p.channels.len() >= crate::engine::MAX_CH {
            continue;
        }
        let id = p.new_id();
        let mut c = crate::project::Channel::new(id, default_inst);
        c.name = if t.name.is_empty() { format!("MIDI {}", ti + 1) } else { t.name.chars().take(24).collect() };
        p.channels.push(c);
        let v: Vec<Note> = mel
            .iter()
            .filter(|x| (x.start as usize) < n)
            .map(|x| {
                let s = x.start.floor();
                Note { s: s as u32, l: (x.len.round() as u32).clamp(1, n as u32 - s as u32), m: x.key, v: x.vel.max(0.05), o: (x.start - s) as f32 }
            })
            .collect();
        pat.notes.insert(id, v);
    }
    p.patterns.push(pat);
    Ok(pid)
}

fn put_vlq(v: &mut Vec<u8>, mut x: u32) {
    let mut buf = [0u8; 5];
    let mut i = 4;
    buf[i] = (x & 0x7f) as u8;
    x >>= 7;
    while x > 0 {
        i -= 1;
        buf[i] = ((x & 0x7f) as u8) | 0x80;
        x >>= 7;
    }
    v.extend_from_slice(&buf[i..]);
}

/// Exporta un patrón a MIDI (formato 1): una pista por canal; la batería va al canal 10.
pub fn export_pattern(p: &Project, pat: &Pattern) -> Vec<u8> {
    const TPQ: u32 = 96;
    let step = TPQ / 4;
    let mut tracks: Vec<Vec<u8>> = vec![];
    // pista de tempo
    let mut t0 = vec![];
    let us = (60_000_000.0 / p.bpm.max(20.0)) as u32;
    t0.extend_from_slice(&[0, 0xff, 0x51, 3, (us >> 16) as u8, (us >> 8) as u8, us as u8]);
    t0.extend_from_slice(&[0, 0xff, 0x58, 4, 4, 2, 24, 8]);
    t0.extend_from_slice(&[0, 0xff, 0x2f, 0]);
    tracks.push(t0);
    let mut mel_ch = 0u8;
    for c in &p.channels {
        let mut ev: Vec<(u32, u8, u8, u8)> = vec![]; // tiempo, estado, nota, vel
        let ch = if c.inst.is_drum() { 9 } else { mel_ch };
        if c.inst.uses_steps() {
            if let Some(a) = pat.steps.get(&c.id) {
                let key = if c.inst == Inst::Sampler { c.smp.root } else { drum_gm(c.inst) };
                for (i, v) in a.iter().enumerate() {
                    if *v > 0.0 {
                        let t = i as u32 * step;
                        ev.push((t, 0x90 | ch, key, ((v * 127.0) as u8).max(1)));
                        ev.push((t + step / 2, 0x80 | ch, key, 0));
                    }
                }
            }
        }
        if let Some(ns) = pat.notes.get(&c.id) {
            for n in ns {
                let t = n.s * step + (n.o * step as f32) as u32;
                ev.push((t, 0x90 | ch, n.m, ((n.v * 127.0) as u8).max(1)));
                ev.push((t + n.l * step - 1, 0x80 | ch, n.m, 0));
            }
        }
        if ev.is_empty() {
            continue;
        }
        if !c.inst.is_drum() {
            mel_ch = (mel_ch + 1) % 16;
            if mel_ch == 9 {
                mel_ch = 10;
            }
        }
        ev.sort_by_key(|e| (e.0, e.1 & 0xf0 == 0x90));
        let mut tr = vec![];
        let name = c.name.as_bytes();
        tr.extend_from_slice(&[0, 0xff, 0x03]);
        put_vlq(&mut tr, name.len() as u32);
        tr.extend_from_slice(name);
        let mut last = 0;
        for (t, s, k, v) in ev {
            put_vlq(&mut tr, t - last);
            last = t;
            tr.extend_from_slice(&[s, k, v]);
        }
        tr.extend_from_slice(&[0, 0xff, 0x2f, 0]);
        tracks.push(tr);
    }
    let mut out = vec![];
    out.extend_from_slice(b"MThd");
    out.extend_from_slice(&6u32.to_be_bytes());
    out.extend_from_slice(&1u16.to_be_bytes());
    out.extend_from_slice(&(tracks.len() as u16).to_be_bytes());
    out.extend_from_slice(&(TPQ as u16).to_be_bytes());
    for t in tracks {
        out.extend_from_slice(b"MTrk");
        out.extend_from_slice(&(t.len() as u32).to_be_bytes());
        out.extend_from_slice(&t);
    }
    out
}

/// Mensaje de un teclado MIDI conectado.
#[derive(Clone, Copy, Debug)]
pub enum Live {
    On(u8, f32),
    Off(u8),
    /// controlador (CC): número y valor 0..1
    Cc(u8, f32),
}

#[cfg(not(target_os = "android"))]
pub mod input {
    use super::Live;
    use std::sync::mpsc::Sender;

    pub struct Conn {
        _c: midir::MidiInputConnection<()>,
        pub name: String,
    }

    pub fn ports() -> Vec<String> {
        let Ok(m) = midir::MidiInput::new("NoName Studio") else { return vec![] };
        m.ports().iter().filter_map(|p| m.port_name(p).ok()).collect()
    }

    /// Conecta el puerto `idx` (o el primero) y manda los mensajes por `tx`; `wake` despierta la interfaz.
    pub fn connect(idx: usize, tx: Sender<Live>, wake: impl Fn() + Send + 'static) -> Result<Conn, String> {
        let mut m = midir::MidiInput::new("NoName Studio").map_err(|e| e.to_string())?;
        m.ignore(midir::Ignore::All);
        let ports = m.ports();
        let port = ports.get(idx).or(ports.first()).ok_or("No hay teclados MIDI conectados.")?;
        let name = m.port_name(port).unwrap_or_else(|_| "MIDI".into());
        let c = m
            .connect(
                port,
                "nns-in",
                move |_, msg, _| {
                    if msg.len() < 3 {
                        return;
                    }
                    let ev = match msg[0] & 0xf0 {
                        0x90 if msg[2] > 0 => Live::On(msg[1], msg[2] as f32 / 127.0),
                        0x80 | 0x90 => Live::Off(msg[1]),
                        0xb0 => Live::Cc(msg[1], msg[2] as f32 / 127.0),
                        _ => return,
                    };
                    let _ = tx.send(ev);
                    wake();
                },
                (),
            )
            .map_err(|e| e.to_string())?;
        Ok(Conn { _c: c, name })
    }
}
