// Prueba del anfitrión CLAP con un plugin de prueba.
//   cargo run --example clap_test -- <carpeta con .clap>
#[cfg(windows)]
fn main() {
    use noname_studio::engine::{render_song_with, Bank, Ev, Proc};
    use noname_studio::plugin_host as host;
    use noname_studio::project::*;
    use std::sync::atomic::{AtomicBool, AtomicU32};

    let dir = std::path::PathBuf::from(std::env::args().nth(1).expect("carpeta"));
    let mut all = true;
    let mut ok = |c: bool, what: &str, d: String| {
        println!("{} {what}: {d}", if c { "OK   " } else { "FALLA" });
        all &= c;
    };
    let (list, errs) = host::scan(&[dir.clone()]);
    ok(list.len() == 2 && errs.is_empty(), "Buscar plugins", format!("{:?}", list.iter().map(|p| (&p.name, p.instrument)).collect::<Vec<_>>()));
    let gain = list.iter().find(|p| !p.instrument).unwrap();
    let sine = list.iter().find(|p| p.instrument).unwrap();

    // efecto + parámetro
    let sh = host::instantiate(&gain.path, &gain.id, 7, 44100.0, "").unwrap();
    ok(sh.params.len() == 1 && sh.params[0].name == "Ganancia", "Parámetros", format!("{:?}", sh.params.iter().map(|p| (&p.name, p.min, p.max)).collect::<Vec<_>>()));
    let mut p = sh.make_proc();
    let (mut l, mut r) = (vec![0.5f32; 256], vec![0.5f32; 256]);
    p.process(&mut l, &mut r, &[]);
    ok((l[10] - 0.5).abs() < 1e-6, "Efecto: procesa audio", format!("0.5 → {}", l[10]));
    p.set_param(1, 0.25);
    let (mut l, mut r) = (vec![0.5f32; 256], vec![0.5f32; 256]);
    p.process(&mut l, &mut r, &[]);
    ok((l[10] - 0.25).abs() < 1e-6, "Automatización de parámetro", format!("ganancia 0.5 → salida {}", l[10]));
    p.stop();
    drop(p);
    let st = sh.save_state();
    let sh2 = host::instantiate(&gain.path, &gain.id, 8, 44100.0, &st).unwrap();
    let mut p2 = sh2.make_proc();
    let (mut l, mut r) = (vec![1.0f32; 64], vec![1.0f32; 64]);
    p2.process(&mut l, &mut r, &[]);
    ok((l[0] - 0.5).abs() < 1e-6, "Guardar y cargar estado", format!("estado «{st}» → ganancia {}", l[0]));
    drop(p2);

    // instrumento
    let si = host::instantiate(&sine.path, &sine.id, 9, 44100.0, "").unwrap();
    let mut ps = si.make_proc();
    let mut out = vec![];
    for b in 0..40 {
        let (mut l, mut r) = (vec![0f32; 256], vec![0f32; 256]);
        let ev = if b == 0 { vec![Ev { at: 0, on: true, note: 81, vel: 1.0 }] } else { vec![] };
        ps.process(&mut l, &mut r, &ev);
        out.extend(l);
    }
    let z = out.windows(2).filter(|w| w[0] <= 0.0 && w[1] > 0.0).count() as f32 / (out.len() as f32 / 44100.0);
    ok((z - 880.0).abs() < 25.0, "Instrumento: nota MIDI", format!("nota 81 → {z:.0} Hz"));
    drop(ps);

    // proyecto completo: canal Plugin con el seno + efecto en el master, exportado sin interfaz
    let mut pr = Project::empty("clap", 120.0);
    pr.limiter.on = false;
    pr.master_comp.on = false;
    let id = pr.new_id();
    let mut c = Channel::new(id, Inst::Plugin);
    c.vol = 1.0;
    c.instrument = Some(PluginSlot { path: sine.path.clone(), id: sine.id.clone(), name: sine.name.clone(), on: true, state: String::new(), uid: 100 });
    c.inserts.push(PluginSlot { path: gain.path.clone(), id: gain.id.clone(), name: gain.name.clone(), on: true, state: st.clone(), uid: 101 });
    pr.channels.push(c);
    let pid = pr.new_id();
    let mut pat = Pattern { id: pid, name: "p".into(), bars: 1, color: PALETTE[0], steps: Default::default(), notes: Default::default(), audio: Default::default() };
    pat.notes.insert(id, vec![Note { s: 0, l: 8, m: 69, v: 1.0, o: 0.0 }]);
    pr.patterns.push(pat);
    pr.playlist.push(Clip { track: 0, bar: 0, pat: pid, off: 0 });
    let slots: Vec<(u32, PluginSlot)> = pr.channels.iter().flat_map(|c| c.inserts.iter().chain(c.instrument.iter()).map(move |s| (c.id, s.clone()))).collect();
    let (a, b) = (AtomicU32::new(0), AtomicBool::new(false));
    let audio = render_song_with(&pr, &Bank::default(), 44100.0, &a, &b, &mut |e| {
        for (ch, s) in &slots {
            let sh = host::instantiate(&s.path, &s.id, s.uid, 44100.0, &s.state).unwrap();
            if s.uid == 100 {
                e.set_instrument(*ch, Some(sh.make_proc()));
            } else {
                e.set_inserts(*ch, vec![sh.make_proc()]);
            }
        }
    });
    let seg = |t0: f32, t1: f32| -> f32 {
        let s = &audio[(t0 * 44100.0) as usize * 2..(t1 * 44100.0) as usize * 2];
        (s.iter().map(|x| x * x).sum::<f32>() / s.len() as f32).sqrt()
    };
    let (on, off) = (seg(0.1, 0.9), seg(1.2, 1.9));
    // seno 0.5 · ganancia 0.5 · paneo centro 0.707 → rms ≈ 0.125
    ok((on - 0.125).abs() < 0.02 && off < 0.001, "Canal de instrumento CLAP + efecto en el motor", format!("rms con nota {on:.3}, sin nota {off:.4}"));

    // interfaz gráfica: se abre la ventana, se procesa un rato y se cierra
    match sh.open_gui("NN Test Gain") {
        Ok(()) => {
            let t = std::time::Instant::now();
            while t.elapsed().as_millis() < 1500 {
                pump();
                sh.poll();
                std::thread::sleep(std::time::Duration::from_millis(15));
            }
            let open = sh.gui_open();
            sh.close_gui();
            ok(open && !sh.gui_open(), "Interfaz gráfica del plugin", "ventana abierta 1.5 s y cerrada".into());
        }
        Err(e) => ok(false, "Interfaz gráfica del plugin", e),
    }
    drop(sh);
    drop(sh2);
    drop(si);
    println!("{}", if all { "TODO OK" } else { "HAY FALLAS" });
}

#[cfg(windows)]
fn pump() {
    use windows_sys::Win32::UI::WindowsAndMessaging::*;
    unsafe {
        let mut m: MSG = std::mem::zeroed();
        while PeekMessageW(&mut m, std::ptr::null_mut(), 0, 0, PM_REMOVE) != 0 {
            TranslateMessage(&m);
            DispatchMessageW(&m);
        }
    }
}

#[cfg(not(windows))]
fn main() {}
