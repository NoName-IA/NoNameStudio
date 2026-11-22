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
    let d = noname_studio::export::export_full(&Project::demo(), &Bank::default(), &out, noname_studio::export::Format::Stems, &pr, &ca, &mut |_| {}).unwrap();
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

    let _ = render_song_with;
    println!("{}", if all { "TODO OK" } else { "HAY FALLAS" });
    if !all {
        std::process::exit(1);
    }
}
