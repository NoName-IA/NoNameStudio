use noname_studio::engine::{Engine, Mode, Status};
use noname_studio::project::Project;
use std::sync::{atomic::Ordering, Arc};
fn main() {
    let p = Project::demo();
    let pat = p.patterns[0].id;
    let st = Arc::new(Status::new());
    let mut e = Engine::new(48000.0, p, st.clone());
    e.start(Mode::Pattern(pat), 0);
    let mut buf = vec![0f32; 1024];
    let mut maxgr = [0f32; 2];
    let mut pk = 0f32;
    for _ in 0..200 {
        e.render(&mut buf);
        pk = buf.iter().fold(pk, |m, x| m.max(x.abs()));
        for k in 0..2 {
            maxgr[k] = maxgr[k].max(f32::from_bits(st.gr[k].load(Ordering::Relaxed)));
        }
    }
    println!("GR compresor máx {:.2} dB · GR limitador máx {:.2} dB · pico salida {:.2} dBFS", maxgr[0], maxgr[1], 20.0 * pk.log10());
}
