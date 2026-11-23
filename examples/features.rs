// Verificación sin interfaz de las funciones nuevas del motor.
//   cargo run --example features -- <carpeta temporal>
use noname_studio::engine::{render_song, render_song_with, Bank, Cmd, Engine, Mode, Status};
use noname_studio::media::{self, Buf};
use noname_studio::project::*;
use std::sync::atomic::{AtomicBool, AtomicU32};
use std::sync::Arc;

const SR: f32 = 44_100.0;

fn rms(a: &[f32]) -> f32 {
    (a.iter().map(|x| x * x).sum::<f32>() / a.len().max(1) as f32).sqrt()
}
fn seg(a: &[f32], t0: f32, t1: f32) -> &[f32] {
    let i0 = ((t0 * SR) as usize * 2).min(a.len());
    let i1 = ((t1 * SR) as usize * 2).min(a.len());
    &a[i0..i1]
}
fn db(x: f32) -> f32 {
    20.0 * x.max(1e-9).log10()
}
/// Frecuencia por cruces por cero (canal izquierdo).
fn freq(a: &[f32]) -> f32 {
    let l: Vec<f32> = a.chunks(2).map(|f| f[0]).collect();
    let z = l.windows(2).filter(|w| w[0] <= 0.0 && w[1] > 0.0).count();
    z as f32 / (l.len() as f32 / SR)
}
fn render(p: &Project, b: &Bank) -> Vec<f32> {
    let (pr, ca) = (AtomicU32::new(0), AtomicBool::new(false));
    render_song(p, b, SR, &pr, &ca)
}
fn ok(cond: bool, what: &str, detail: String) -> bool {
    println!("{} {what}: {detail}", if cond { "OK   " } else { "FALLA" });
    cond
}

fn sine(f: f32, secs: f32, amp: f32) -> Buf {
    let n = (secs * SR) as usize;
    Buf { ch: 1, sr: SR, data: (0..n).map(|i| (i as f32 / SR * f * std::f32::consts::TAU).sin() * amp).collect() }
}

fn main() {
    let dir = std::path::PathBuf::from(std::env::args().nth(1).unwrap_or_else(|| "prueba_features".into()));
    let _ = std::fs::create_dir_all(dir.join("samples"));
    let mut all = true;

    // 1. demo
    let demo = Project::demo();
    let a = render(&demo, &Bank::default());
    let nan = a.iter().any(|x| !x.is_finite());
    let pk = a.iter().fold(0f32, |m, x| m.max(x.abs()));
    all &= ok(!nan && pk > 0.3 && pk <= 1.0, "Beat de ejemplo", format!("{:.1} s, pico {:.2}, rms {:.1} dB", a.len() as f32 / 2.0 / SR, pk, db(rms(&a))));
    let (l, _) = noname_studio::loudness::measure(&a, SR);
    all &= ok(l > -30.0 && l < -3.0, "LUFS del beat", format!("{l:.1} LUFS"));

    // 2. LUFS de referencia: seno 1 kHz a -20 dBFS en ambos canales ≈ -20 LUFS
    let s: Vec<f32> = (0..(SR * 5.0) as usize).flat_map(|i| {
        let v = (i as f32 / SR * 1000.0 * std::f32::consts::TAU).sin() * 0.1;
        [v, v]
    }).collect();
    let (l, _) = noname_studio::loudness::measure(&s, SR);
    all &= ok((l + 20.0).abs() < 0.6, "Medidor LUFS (seno 1 kHz -20 dBFS)", format!("{l:.2} LUFS"));

    // 3. sampler: seno 440 Hz como sample, nota base 69 (La4); tocar 81 → 880 Hz
    let b440 = sine(440.0, 1.0, 0.5);
    media::write_wav(&dir.join("samples").join("seno440.wav"), &b440).unwrap();
    let mut p = Project::empty("sampler", 120.0);
    p.limiter.on = false;
    p.master_comp.on = false;
    let id = p.new_id();
    let mut c = Channel::new(id, Inst::Sampler);
    c.smp.file = "seno440.wav".into();
    c.smp.root = 69;
    c.smp.oneshot = false;
    c.vol = 1.0;
    p.channels.push(c);
    let pid = p.new_id();
    let mut pat = Pattern { id: pid, name: "s".into(), bars: 1, color: PALETTE[0], steps: Default::default(), notes: Default::default(), audio: Default::default() };
    pat.notes.insert(id, vec![Note { s: 0, l: 4, m: 81, v: 1.0, o: 0.0 }]);
    pat.steps.insert(id, { let mut v = vec![0.0; 16]; v[8] = 1.0; v });
    p.patterns.push(pat);
    p.playlist.push(Clip { track: 0, bar: 0, pat: pid });
    let mut bank = Bank::default();
    media::ensure(&p, &dir, &mut bank.files);
    let a = render(&p, &bank);
    let f1 = freq(seg(&a, 0.05, 0.4));
    let f2 = freq(seg(&a, 1.01, 1.11));
    all &= ok((f1 - 880.0).abs() < 15.0, "NN Sampler transpone (nota 81)", format!("{f1:.0} Hz (esperado 880)"));
    all &= ok((f2 - 440.0).abs() < 10.0, "NN Sampler en la rejilla de pasos", format!("{f2:.0} Hz (esperado 440)"));

    // 4. pitch sin cambiar tempo (+12 st) y estiramiento ×2 sin cambiar tono
    let up = media::transform(&b440, 1.0, 12.0);
    let st = media::transform(&b440, 2.0, 0.0);
    let fu = { let v: Vec<f32> = up.data.iter().flat_map(|x| [*x, *x]).collect(); freq(&v[..(SR as usize)]) };
    let fs = { let v: Vec<f32> = st.data.iter().flat_map(|x| [*x, *x]).collect(); freq(&v) };
    all &= ok((up.secs() - 1.0).abs() < 0.05 && (fu - 880.0).abs() < 20.0, "Cambio de tono sin cambiar duración", format!("{:.2} s, {fu:.0} Hz", up.secs()));
    all &= ok((st.secs() - 2.0).abs() < 0.05 && (fs - 440.0).abs() < 12.0, "Time-stretch ×2 sin cambiar tono", format!("{:.2} s, {fs:.0} Hz", st.secs()));

    // 5. clip de audio ajustado al tempo: loop de 2 s grabado a 120 bpm (1 compás) en un proyecto a 60 bpm → 4 s
    let b2 = sine(330.0, 2.0, 0.5);
    media::write_wav(&dir.join("samples").join("loop120.wav"), &b2).unwrap();
    let mut p = Project::empty("clip", 60.0);
    p.limiter.on = false;
    p.master_comp.on = false;
    let ch = p.new_id();
    let mut c = Channel::new(ch, Inst::Audio);
    c.vol = 1.0;
    p.channels.push(c);
    let cid = p.new_id();
    p.audio_clips.push(AudioClip { id: cid, ch, track: 0, start: 0, len: 16, off: 0.0, takes: vec!["loop120.wav".into()], active: 0, gain: 1.0, pitch: 0.0, bpm: 120.0, fade_in: 0.0, fade_out: 0.0, reverse: false });
    let mut bank = Bank::default();
    media::ensure(&p, &dir, &mut bank.files);
    let a = render(&p, &bank);
    let r_mid = rms(seg(&a, 3.0, 3.8));
    let r_after = rms(seg(&a, 4.3, 5.0));
    let fc = freq(seg(&a, 0.5, 3.5));
    all &= ok(r_mid > 0.1 && r_after < 0.01 && (fc - 330.0).abs() < 10.0, "Clip de audio estirado al tempo", format!("suena hasta los 4 s (rms {:.2}), luego {:.3}; {fc:.0} Hz", r_mid, r_after));

    // 6. automatización de volumen: de 0 a 1 en 4 compases
    let mut p = Project::demo();
    p.limiter.on = false;
    p.master_comp.on = false;
    p.vinyl = 0.0;
    let r = p.channels.iter().find(|c| c.inst == Inst::Rhodes).unwrap().id;
    for c in p.channels.iter_mut() {
        c.solo = c.id == r;
        c.rev = 0.0;
    }
    let aid = p.new_id();
    p.autos.push(AutoClip { id: aid, track: 5, start: 0, len: 64, target: AutoTarget::Vol(r), pts: vec![[0.0, 0.0], [1.0, 1.0]] });
    let a = render(&p, &Bank::default());
    let sec = p.step_secs() as f32 * 16.0;
    let (q1, q4) = (rms(seg(&a, 0.0, sec * 0.8)), rms(seg(&a, sec * 3.0, sec * 4.0)));
    all &= ok(q4 > q1 * 4.0, "Automatización de volumen", format!("compás 1 {:.1} dB → compás 4 {:.1} dB", db(q1), db(q4)));

    // 7. sidechain: pad sostenido que se agacha con el kick
    let mut p = Project::empty("sc", 120.0);
    p.limiter.on = false;
    p.master_comp.on = false;
    let k = p.new_id();
    p.channels.push(Channel::new(k, Inst::Kick));
    let pd = p.new_id();
    let mut c = Channel::new(pd, Inst::Pad);
    c.vol = 1.0;
    c.comp = CompP::ducking(k);
    p.channels.push(c);
    p.channel_mut(k).unwrap().mute = false;
    let pid = p.new_id();
    let mut pat = Pattern { id: pid, name: "p".into(), bars: 2, color: PALETTE[0], steps: Default::default(), notes: Default::default(), audio: Default::default() };
    pat.steps.insert(k, (0..32).map(|i| if i % 4 == 0 { 1.0 } else { 0.0 }).collect());
    pat.notes.insert(pd, vec![Note { s: 0, l: 32, m: 60, v: 1.0, o: 0.0 }]);
    p.patterns.push(pat);
    p.playlist.push(Clip { track: 0, bar: 0, pat: pid });
    // solo el pad (el kick sigue disparando el sidechain aunque esté silenciado en la mezcla: se usa su volumen)
    p.channel_mut(k).unwrap().vol = 0.0001;
    let with = render(&p, &Bank::default());
    p.channel_mut(pd).unwrap().comp = CompP::channel();
    let without = render(&p, &Bank::default());
    let beat = p.step_secs() as f32 * 4.0;
    let hit = |a: &[f32]| rms(seg(a, beat * 2.0 + 0.01, beat * 2.0 + 0.06));
    let gap = |a: &[f32]| rms(seg(a, beat * 2.0 + 0.35, beat * 2.0 + 0.45));
    let duck = db(hit(&with)) - db(hit(&without));
    let rec = db(gap(&with)) - db(gap(&without));
    // con volumen casi cero el kick no dispara: se repite con el kick audible y se compara el pad solo
    let _ = (duck, rec);
    p.channel_mut(k).unwrap().vol = 0.9;
    p.channel_mut(pd).unwrap().comp = CompP::ducking(k);
    let mut e = Engine::new(SR, p.clone(), Arc::new(Status::new()));
    e.offline = true;
    e.start(Mode::Song, 0);
    let mut buf = vec![0f32; 2048];
    let mut out = vec![];
    while out.len() < (beat * 4.0 * SR) as usize * 2 {
        e.render(&mut buf);
        out.extend_from_slice(&buf);
    }
    let mut q = p.clone();
    q.channel_mut(pd).unwrap().comp = CompP::channel();
    let mut q2 = q.clone();
    for c in q2.channels.iter_mut() {
        c.solo = c.id == pd;
    }
    let mut p2 = p.clone();
    for c in p2.channels.iter_mut() {
        c.solo = c.id == pd;
    }
    let pad_duck = render(&p2, &Bank::default());
    let pad_flat = render(&q2, &Bank::default());
    let d = db(hit(&pad_duck)) - db(hit(&pad_flat));
    all &= ok(d < -6.0, "Sidechain (pad se agacha con el kick)", format!("{d:.1} dB en el golpe"));

    // 8. arpegiador: acorde de 3 notas, 1 compás → 16 notas a 1/16
    let mut p = Project::empty("arp", 120.0);
    p.limiter.on = false;
    p.master_comp.on = false;
    let s = p.new_id();
    let mut c = Channel::new(s, Inst::Bell);
    c.arp = ArpP { on: true, mode: 0, rate: 1, oct: 1, gate: 0.5 };
    p.channels.push(c);
    let pid = p.new_id();
    let mut pat = Pattern { id: pid, name: "a".into(), bars: 1, color: PALETTE[0], steps: Default::default(), notes: Default::default(), audio: Default::default() };
    pat.notes.insert(s, vec![Note { s: 0, l: 16, m: 60, v: 1.0, o: 0.0 }, Note { s: 0, l: 16, m: 64, v: 1.0, o: 0.0 }, Note { s: 0, l: 16, m: 67, v: 1.0, o: 0.0 }]);
    p.patterns.push(pat);
    p.playlist.push(Clip { track: 0, bar: 0, pat: pid });
    let a = render(&p, &Bank::default());
    let step = p.step_secs() as f32;
    let fs: Vec<f32> = (0..4).map(|i| freq(seg(&a, i as f32 * step + 0.01, i as f32 * step + 0.11))).collect();
    let exp = [261.6, 329.6, 392.0, 261.6];
    let good = fs.iter().zip(exp).all(|(f, e)| (f - e).abs() < e * 0.04);
    all &= ok(good, "Arpegiador (sube)", format!("{:?} Hz", fs.iter().map(|f| f.round()).collect::<Vec<_>>()));

    // 9. bucle en la playlist: la posición vuelve al inicio
    let mut p = Project::demo();
    p.loop_on = true;
    p.loop_a = 2;
    p.loop_b = 3;
    let st = Arc::new(Status::new());
    let mut e = Engine::new(SR, p.clone(), st.clone());
    e.handle(Cmd::Play { mode: Mode::Song, from_step: 32 });
    let mut buf = vec![0f32; 1024];
    let mut maxp = 0f64;
    let total = (p.step_secs() * 16.0 * 3.0 * SR as f64) as usize;
    let mut n = 0;
    while n < total {
        e.render(&mut buf);
        n += 512;
        maxp = maxp.max(st.pos());
    }
    all &= ok(maxp < 48.0 && maxp > 40.0, "Bucle de la playlist", format!("posición máxima {maxp:.1} pasos tras 3 compases (región 32–48)"));

    // 10. MIDI: exportar e importar el patrón de acordes
    let demo = Project::demo();
    let chords = demo.patterns.iter().find(|p| p.name == "Acordes").unwrap();
    let bytes = noname_studio::midi::export_pattern(&demo, chords);
    let mf = noname_studio::midi::read(&bytes).unwrap();
    let n_in: usize = chords.notes.values().map(|v| v.len()).sum();
    let n_out: usize = mf.tracks.iter().map(|t| t.notes.len()).sum();
    all &= ok(n_in == n_out && mf.bpm.is_some_and(|b| (b - 88.0).abs() < 0.5), "MIDI exportar → importar", format!("{n_in} notas → {n_out}, tempo {:?}", mf.bpm));
    let mut q = Project::demo();
    let pid = noname_studio::midi::import(&mut q, &mf, "midi", Inst::Rhodes).unwrap();
    all &= ok(q.pattern(pid).is_some_and(|p| p.notes.values().map(|v| v.len()).sum::<usize>() == n_in), "Importar MIDI como patrón", "notas en el patrón nuevo".into());

    // 11. stems
    let (pr, ca) = (AtomicU32::new(0), AtomicBool::new(false));
    let out = dir.join("stems");
    let d = noname_studio::export::export_full(&Project::demo(), &Bank::default(), &out, noname_studio::export::Format::Stems, &Default::default(), &pr, &ca, &mut |_| {}).unwrap();
    all &= ok(d.files == Project::demo().channels.len(), "Exportar stems", format!("{} archivos en {}", d.files, d.path.display()));

    // 12. plantillas
    for (i, n) in noname_studio::templates::TEMPLATES.iter().enumerate() {
        let p = noname_studio::templates::build(i);
        if p.song_bars() == 0 {
            all &= ok(i == 0, &format!("Plantilla {n}"), "vacía".into());
            continue;
        }
        let a = render(&p, &Bank::default());
        let (l, _) = noname_studio::loudness::measure(&a, SR);
        all &= ok(l > -35.0 && a.iter().all(|x| x.is_finite()), &format!("Plantilla {n}"), format!("{} compases, {l:.1} LUFS", p.song_bars()));
    }

    // 13. transitorios del slicer: 4 golpes
    let mut d = vec![0f32; (SR * 2.0) as usize];
    for k in 0..4 {
        let s0 = (k as f32 * 0.5 * SR) as usize;
        for i in 0..2000 {
            d[s0 + i] = (i as f32 * 0.3).sin() * (-(i as f32) / 400.0).exp();
        }
    }
    let on = media::onsets(&Buf { ch: 1, sr: SR, data: d }, 0.5, 16);
    all &= ok(on.len() == 4, "Detección de golpes (slicer)", format!("{:?}", on.iter().map(|x| (x * 100.0).round() / 100.0).collect::<Vec<_>>()));
    all &= ok((media::guess_bpm("loop 92bpm", 2.6) - 92.0).abs() < 0.1 && (media::guess_bpm("x", 2.0) - 120.0).abs() < 0.1, "Adivinar tempo de un loop", format!("{} / {}", media::guess_bpm("loop 92bpm", 2.6), media::guess_bpm("x", 2.0)));

    // 14. el vinilo no suena con la reproducción detenida
    let mut e = Engine::new(SR, Project::demo(), Arc::new(Status::new()));
    let mut b = vec![0f32; 2048];
    let mut pk = 0f32;
    for _ in 0..40 {
        e.render(&mut b);
        pk = b.iter().fold(pk, |m, x| m.max(x.abs()));
    }
    let mut q = Project::demo();
    q.playlist.clear();
    for c in q.channels.iter_mut() {
        c.mute = true;
    }
    let mut e2 = Engine::new(SR, q, Arc::new(Status::new()));
    e2.start(Mode::Pattern(0), 0);
    let mut pk2 = 0f32;
    for _ in 0..40 {
        e2.render(&mut b);
        pk2 = b.iter().fold(pk2, |m, x| m.max(x.abs()));
    }
    all &= ok(pk == 0.0 && pk2 > 0.0005, "Vinilo solo al reproducir", format!("detenido: pico {pk:.5} · reproduciendo: pico {pk2:.4}"));

    // 15. escucharse: tono de 440 Hz entrando a 44.1 kHz, salida a 48 kHz
    let st = Arc::new(Status::new());
    st.monitor.sr.store(44_100, std::sync::atomic::Ordering::Relaxed);
    st.monitor.on.store(true, std::sync::atomic::Ordering::Relaxed);
    st.monitor.gain.store(1.0f32.to_bits(), std::sync::atomic::Ordering::Relaxed);
    let mut q = Project::empty("m", 120.0);
    q.channels.push(Channel::new(1, Inst::Kick));
    let mut e = Engine::new(48_000.0, q, st.clone());
    let mut out = vec![];
    let mut fase = 0usize;
    let mut b = vec![0f32; 1024];
    for _ in 0..200 {
        // el micrófono entrega ~470 muestras por cada bloque de 512 cuadros de salida
        if let Ok(mut c) = st.monitor.cola.lock() {
            for _ in 0..470 {
                c.push_back((fase as f32 / 44_100.0 * 440.0 * std::f32::consts::TAU).sin() * 0.5);
                fase += 1;
            }
        }
        e.render(&mut b);
        out.extend_from_slice(&b);
    }
    let tramo = &out[out.len() / 2..];
    let l: Vec<f32> = tramo.chunks(2).map(|f| f[0]).collect();
    let z = l.windows(2).filter(|w| w[0] <= 0.0 && w[1] > 0.0).count() as f32 / (l.len() as f32 / 48_000.0);
    let pk = l.iter().fold(0f32, |m, x| m.max(x.abs()));
    let saltos = l.windows(2).filter(|w| (w[1] - w[0]).abs() > 0.2).count();
    all &= ok((z - 440.0).abs() < 8.0 && pk > 0.4 && saltos == 0, "Escucharse al grabar (44.1 → 48 kHz)", format!("{z:.0} Hz, pico {pk:.2}, saltos {saltos}"));

    // 16. autotune en vivo: un La desafinado (450 Hz) sale como La (440 Hz)
    st.monitor.tune_on.store(true, std::sync::atomic::Ordering::Relaxed);
    st.monitor.tune_speed.store(0.0f32.to_bits(), std::sync::atomic::Ordering::Relaxed);
    st.monitor.tune_key.store(9, std::sync::atomic::Ordering::Relaxed);
    st.monitor.tune_scale.store(0, std::sync::atomic::Ordering::Relaxed);
    let mut out = vec![];
    let mut fase = 0usize;
    for _ in 0..300 {
        if let Ok(mut c) = st.monitor.cola.lock() {
            for _ in 0..470 {
                c.push_back((fase as f32 / 44_100.0 * 450.0 * std::f32::consts::TAU).sin() * 0.5);
                fase += 1;
            }
        }
        e.render(&mut b);
        out.extend_from_slice(&b);
    }
    let l: Vec<f32> = out[out.len() / 2..].chunks(2).map(|f| f[0]).collect();
    let z = l.windows(2).filter(|w| w[0] <= 0.0 && w[1] > 0.0).count() as f32 / (l.len() as f32 / 48_000.0);
    all &= ok((z - 440.0).abs() < 4.0, "Autotune en vivo (450 Hz → La 440)", format!("{z:.1} Hz"));
    let t0 = std::time::Instant::now();
    let mut tl = noname_studio::fx::TuneLive::new(48_000.0);
    let tp = TuneP { on: true, key: 9, scale: 0, speed: 0.0, mix: 1.0, gain: 1.0, mask: 0, estilo: 0, algoritmo: 0, mezcla: 1.0 };
    let mut acc = 0f32;
    for i in 0..48_000 {
        acc += tl.process((i as f32 / 48_000.0 * 450.0 * std::f32::consts::TAU).sin() * 0.5, &tp);
    }
    let ms = t0.elapsed().as_secs_f32() * 1000.0;
    all &= ok(ms < 150.0 && acc.is_finite(), "Autotune en vivo: costo", format!("{ms:.0} ms por cada segundo de voz"));

    // 17. estilos: «Ardilla» sube una octava y «Dúo» agrega la octava grave
    let x: Vec<f32> = (0..44_100).map(|i| (i as f32 / 44_100.0 * 220.0 * std::f32::consts::TAU).sin() * 0.5).collect();
    let base = TuneP { on: true, key: 9, scale: 0, speed: 0.0, mix: 1.0, gain: 1.0, mask: scale_mask(9, 1), estilo: 5, algoritmo: 0, mezcla: 1.0 };
    let ard = noname_studio::fx::autotune(&x, 44_100.0, &base);
    let fz = |v: &[f32]| v.windows(2).filter(|w| w[0] <= 0.0 && w[1] > 0.0).count() as f32 / (v.len() as f32 / 44_100.0);
    let mut fs: Vec<f32> = (0..12).filter_map(|k| noname_studio::fx::yin(&ard.out[12_000 + k * 2_000..14_048 + k * 2_000], 44_100.0)).collect();
    fs.sort_by(|a, b| a.total_cmp(b));
    println!("      ardilla, tonos medidos: {:?}", fs.iter().map(|f| f.round()).collect::<Vec<_>>());
    let f_ard = fs.get(fs.len() / 2).copied().unwrap_or(0.0);
    let _ = fz;
    // voz de prueba con armónicos (como una voz real): el dúo debe sumar la octava grave (110 Hz)
    let voz: Vec<f32> = (0..44_100).map(|i| (1..8).map(|h| (i as f32 / 44_100.0 * 220.0 * h as f32 * std::f32::consts::TAU).sin() * 0.35 / h as f32).sum()).collect();
    let duo = noname_studio::fx::autotune(&voz, 44_100.0, &TuneP { estilo: 3, ..base });
    let mut fd: Vec<f32> = (0..10).filter_map(|k| noname_studio::fx::yin(&duo.out[12_000 + k * 2_000..14_048 + k * 2_000], 44_100.0)).collect();
    fd.sort_by(|a, b| a.total_cmp(b));
    let f_duo = fd.get(fd.len() / 2).copied().unwrap_or(0.0);
    println!("      dúo, tonos medidos: {:?}", fd.iter().map(|f| f.round()).collect::<Vec<_>>());
    let sube = if (f_duo - 110.0).abs() < 4.0 { 2.0 } else { 0.0 };
    all &= ok((f_ard - 440.0).abs() < 4.0 && sube > 1.5, "Estilos de NN Tune (ardilla y dúo)", format!("ardilla {f_ard:.0} Hz · dúo: tono de base {f_duo:.0} Hz (esperado 110)"));

    // 18. seguir los acordes: se canta un La (450 Hz) sobre Do mayor → la voz va a Sol (392 Hz)
    st.monitor.seguir.store(true, std::sync::atomic::Ordering::Relaxed);
    st.monitor.acorde.store((1 << 0) | (1 << 4) | (1 << 7), std::sync::atomic::Ordering::Relaxed);
    let mut out = vec![];
    for _ in 0..300 {
        if let Ok(mut c) = st.monitor.cola.lock() {
            for _ in 0..470 {
                c.push_back((fase as f32 / 44_100.0 * 450.0 * std::f32::consts::TAU).sin() * 0.5);
                fase += 1;
            }
        }
        e.render(&mut b);
        out.extend_from_slice(&b);
    }
    let l: Vec<f32> = out[out.len() / 2..].chunks(2).map(|f| f[0]).collect();
    let mut fs: Vec<f32> = (0..8).filter_map(|k| noname_studio::fx::yin(&l[k * 4_000..k * 4_000 + 2_048], 48_000.0)).collect();
    fs.sort_by(|a, b| a.total_cmp(b));
    let fa = fs.get(fs.len() / 2).copied().unwrap_or(0.0);
    let cant = f32::from_bits(st.monitor.cantada.load(std::sync::atomic::Ordering::Relaxed));
    all &= ok((fa - 392.0).abs() < 5.0 && (cant - 69.3).abs() < 0.3, "Seguir los acordes (La sobre Do mayor → Sol)", format!("sale {fa:.0} Hz; la pantalla muestra nota cantada {cant:.2}"));

    // 19. algoritmo de baja demora y gran armonía (tres voces)
    let x450: Vec<f32> = (0..44_100).map(|i| (i as f32 / 44_100.0 * 450.0 * std::f32::consts::TAU).sin() * 0.5).collect();
    let bd = noname_studio::fx::autotune(&x450, 44_100.0, &TuneP { estilo: 0, algoritmo: 1, mask: scale_mask(9, 1), ..base });
    let mut fb: Vec<f32> = (0..8).filter_map(|k| noname_studio::fx::yin(&bd.out[12_000 + k * 3_000..14_048 + k * 3_000], 44_100.0)).collect();
    fb.sort_by(|a, b| a.total_cmp(b));
    let f_bd = fb.get(fb.len() / 2).copied().unwrap_or(0.0);
    let ga = noname_studio::fx::autotune(&voz, 44_100.0, &TuneP { estilo: 6, ..base });
    // en La menor: la tercera (Do, 261.6 Hz) y la quinta (Mi, 330 Hz) sobre La 220
    let energia = |v: &[f32], f: f32| {
        let (mut c, mut s) = (0f32, 0f32);
        for (i, x) in v.iter().enumerate() {
            let a = i as f32 / 44_100.0 * f * std::f32::consts::TAU;
            c += x * a.cos();
            s += x * a.sin();
        }
        (c * c + s * s).sqrt() / v.len() as f32
    };
    let (t3, t5) = (energia(&ga.out[10_000..40_000], 261.6) / energia(&voz[10_000..40_000], 261.6).max(1e-6), energia(&ga.out[10_000..40_000], 329.6) / energia(&voz[10_000..40_000], 329.6).max(1e-6));
    all &= ok((f_bd - 440.0).abs() < 4.0 && t3 > 5.0 && t5 > 5.0, "Baja demora y gran armonía", format!("baja demora {f_bd:.0} Hz · tercera ×{t3:.0} · quinta ×{t5:.0}"));

    // 20. precisión: una voz desafinada (+25 ¢) con vibrato de ±40 ¢ sale clavada en La 440,
    // con todos los estilos sin armonía e intensidad media, en los dos algoritmos
    let sr = 48_000.0f32;
    let mut fase = 0f32;
    let voz: Vec<f32> = (0..(sr as usize * 2))
        .map(|i| {
            let t = i as f32 / sr;
            let f = 440.0 * 2f32.powf((25.0 + 40.0 * (t * 5.5 * std::f32::consts::TAU).sin()) / 1200.0);
            fase += f / sr;
            let w = fase * std::f32::consts::TAU;
            (w.sin() * 0.5 + (2.0 * w).sin() * 0.25 + (3.0 * w).sin() * 0.12) * 0.6
        })
        .collect();
    let mut peor = 0f32;
    let mut detalle = vec![];
    for (estilo, alg) in [(0u8, 0u8), (0, 1), (1, 0), (2, 0), (2, 1)] {
        let tp = TuneP { on: true, key: 9, scale: 0, speed: 0.45, mix: 1.0, gain: 1.0, mask: scale_mask(9, 0), estilo, algoritmo: alg, mezcla: 1.0 };
        let o = noname_studio::fx::autotune(&voz, sr, &tp).out;
        let mut maxc = 0f32;
        for k in 6..((o.len() - 2048) / 1024) {
            if let Some(h) = noname_studio::fx::yin(&o[k * 1024..k * 1024 + 2048], sr) {
                maxc = maxc.max((1200.0 * (h / 440.0).log2()).abs());
            }
        }
        peor = peor.max(maxc);
        detalle.push(format!("{}{} ±{maxc:.0}¢", noname_studio::fx::ESTILOS[estilo as usize].nombre, if alg == 1 { " (baja demora)" } else { "" }));
    }
    all &= ok(peor < 12.0, "Precisión con vibrato (+25 ¢ ± 40 ¢ → La 440)", detalle.join(" · "));

    // 21. Retune: dos notas (La y Mi con vibrato); se detectan, la primera se sube a Do y se aplana
    let sr = 44_100.0f32;
    let mut fase = 0f32;
    let voz: Vec<f32> = (0..(sr as usize * 2))
        .map(|i| {
            let t = i as f32 / sr;
            if (0.95..1.05).contains(&t) {
                return 0.0;
            }
            let base = if t < 1.0 { 440.0 } else { 329.63 };
            let f = base * 2f32.powf(30.0 * (t * 5.5 * std::f32::consts::TAU).sin() / 1200.0);
            fase += f / sr;
            let w = fase * std::f32::consts::TAU;
            (w.sin() * 0.5 + (2.0 * w).sin() * 0.25 + (3.0 * w).sin() * 0.12) * 0.6
        })
        .collect();
    let cont = noname_studio::fx::contorno(&voz, sr);
    let mut notas = noname_studio::fx::notas_de(&cont);
    let detectadas: Vec<String> = notas.iter().map(|n| format!("{:.2}", n.centro)).collect();
    if let Some(n) = notas.first_mut() {
        n.objetivo = 72.0;
    }
    let o = noname_studio::fx::retune(&voz, sr, &cont, &notas, 1.0);
    let medir_tramo = |a: f32, b: f32| -> (f32, f32) {
        let mut v = vec![];
        let mut k = (a * sr) as usize;
        while k + 2048 < (b * sr) as usize {
            if let Some(h) = noname_studio::fx::yin(&o[k..k + 2048], sr) {
                v.push(1200.0 * (h / 440.0).log2() / 100.0 + 69.0);
            }
            k += 1024;
        }
        let m = v.iter().sum::<f32>() / v.len().max(1) as f32;
        let dev = v.iter().fold(0f32, |d, x| d.max((x - m).abs())) * 100.0;
        (m, dev)
    };
    let (m1, d1) = medir_tramo(0.2, 0.85);
    let (m2, d2) = medir_tramo(1.3, 1.85);
    all &= ok(notas.len() == 2 && (m1 - 72.0).abs() < 0.1 && d1 < 10.0 && (m2 - 64.0).abs() < 0.1, "Retune (2 notas; La → Do, aplanado)", format!("notas detectadas {detectadas:?} · 1ª sale {m1:.2} (±{d1:.0}¢) · 2ª sale {m2:.2} (±{d2:.0}¢)"));

    // 22. pistas de la playlist: silencio, solo, volumen y paneo
    let mut base = Project::demo();
    base.vinyl = 0.0;
    let pico = |a: &[f32], lado: usize| a.chunks(2).fold(0f32, |m, f| m.max(f[lado].abs()));
    let a0 = render(&base, &Bank::default());
    let mut q = base.clone();
    for t in 0..q.tracks.len() {
        q.pista_mut(t).mute = true;
    }
    let a_mute = render(&q, &Bank::default());
    let mut q = base.clone();
    for t in 0..q.tracks.len() {
        q.pista_mut(t).vol = 0.5;
    }
    let a_vol = render(&q, &Bank::default());
    let mut q = base.clone();
    for t in 0..q.tracks.len() {
        q.pista_mut(t).pan = -1.0;
    }
    q.limiter.on = false;
    let a_pan = render(&q, &Bank::default());
    let rms = |a: &[f32]| (a.iter().map(|v| v * v).sum::<f32>() / a.len() as f32).sqrt();
    let dv = 20.0 * (rms(&a_vol) / rms(&a0)).log10();
    let (pl, pr) = (pico(&a_pan, 0), pico(&a_pan, 1));
    all &= ok(pico(&a_mute, 0) < 1e-4 && dv < -3.0 && dv > -9.0 && pr < pl * 0.35, "Pistas: silencio, volumen y paneo", format!("silenciadas: pico {:.5} · volumen 50 %: {dv:.1} dB · paneo izquierda: izq {pl:.2} / der {pr:.2}", pico(&a_mute, 0)));

    // 23. nivelador de la grabación: una voz bajita queda tan fuerte como al escucharte (y sin cortes)
    let bajita: Vec<f32> = (0..48_000).map(|i| (i as f32 / 48_000.0 * 220.0 * std::f32::consts::TAU).sin() * 0.05).collect();
    let niv = noname_studio::fx::nivelar(&bajita, 48_000.0);
    let pk_in = 0.05f32;
    let pk_out = niv[24_000..].iter().fold(0f32, |m, v| m.max(v.abs()));
    let fuerte: Vec<f32> = bajita.iter().map(|v| v * 18.0).collect();
    let pk_fuerte = noname_studio::fx::nivelar(&fuerte, 48_000.0).iter().fold(0f32, |m, v| m.max(v.abs()));
    all &= ok(pk_out > 0.3 && pk_fuerte <= 1.0, "Nivelador de la grabación", format!("voz bajita: pico {pk_in:.2} → {pk_out:.2} · voz muy fuerte (pico 0,90): sale {pk_fuerte:.2} sin pasarse de 1"));

    // 24. armonías: La cantado en Do mayor → tercera arriba Do, quinta Mi, octava abajo La grave
    let sr = 44_100.0f32;
    let mut fase = 0f32;
    let la: Vec<f32> = (0..(sr as usize * 2)).map(|i| {
        let t = i as f32 / sr;
        let f = 440.0 * 2f32.powf(20.0 * (t * 5.0 * std::f32::consts::TAU).sin() / 1200.0);
        fase += f / sr;
        let w = fase * std::f32::consts::TAU;
        (w.sin() * 0.5 + (2.0 * w).sin() * 0.25 + (3.0 * w).sin() * 0.12) * 0.6
    }).collect();
    let mascara = scale_mask(0, 0);
    let tono = |o: &[f32]| -> f32 {
        let mut v = vec![];
        let mut k = (0.3 * sr) as usize;
        while k + 2048 < (1.7 * sr) as usize {
            if let Some(h) = noname_studio::fx::yin(&o[k..k + 2048], sr) {
                v.push(69.0 + 12.0 * (h / 440.0).log2());
            }
            k += 2048;
        }
        v.sort_by(|a, b| a.total_cmp(b));
        v.get(v.len() / 2).copied().unwrap_or(0.0)
    };
    let t3 = tono(&noname_studio::fx::armonia(&la, sr, mascara, 2, None));
    let t5 = tono(&noname_studio::fx::armonia(&la, sr, mascara, 4, None));
    let t8 = tono(&noname_studio::fx::armonia(&la, sr, mascara, 0, Some(-12)));
    all &= ok((t3 - 72.0).abs() < 0.15 && (t5 - 76.0).abs() < 0.15 && (t8 - 57.0).abs() < 0.15, "Armonías (La en Do mayor)", format!("tercera arriba {t3:.2} (Do 72) · quinta {t5:.2} (Mi 76) · octava abajo {t8:.2} (La 57)"));

    let _ = render_song_with;
    println!("{}", if all { "TODO OK" } else { "HAY FALLAS" });
    if !all {
        std::process::exit(1);
    }
}
