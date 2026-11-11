//! FirstFrame cost and engine parity: run the SDK-compiled app with scripted
//! taps through an in-memory host, report instructions and wall time per
//! presented frame, and (on x86-64 Linux) require the JIT to match the
//! interpreter's complete outcome and every frame hash in both dispatch modes.
//!
//! usage: firstframe_bench DIR [TAPS]   (DIR holds FirstFrame-Os and the providers)
use sable::{
    engine::{Outcome, Prepared},
    macho::Image,
    ui::{Event, Host, sha256},
};
use std::{collections::VecDeque, time::Instant};

struct Scripted {
    events: VecDeque<Event>,
    frames: Vec<[u8; 32]>,
}
impl Host for Scripted {
    fn scene_delegate(&mut self) -> sable::Result<String> {
        Ok("SceneDelegate".into())
    }
    fn present(&mut self, pixels: &[u8]) -> sable::Result<()> {
        self.frames.push(sha256(pixels));
        Ok(())
    }
    fn wait_event(&mut self) -> sable::Result<Event> {
        self.events
            .pop_front()
            .ok_or_else(|| "script exhausted".into())
    }
}

fn script(taps: usize) -> VecDeque<Event> {
    let mut events: VecDeque<_> = (0..taps)
        .map(|n| Event::TouchBegan {
            x: if n % 2 == 0 { 40.0 } else { 350.0 },
            y: 400.0,
        })
        .collect();
    events.push_back(Event::Quit);
    events
}

fn run(prepared: &Prepared<'_>, taps: usize) -> (Outcome, Vec<[u8; 32]>, f64) {
    let mut host = Scripted {
        events: script(taps),
        frames: Vec::new(),
    };
    let started = Instant::now();
    let outcome = prepared.run_ui(1_000_000_000, &mut host).unwrap();
    let seconds = started.elapsed().as_secs_f64();
    assert_eq!(outcome.exit, 0);
    assert_eq!(host.frames.len(), taps + 1);
    (outcome, host.frames, seconds)
}

fn main() {
    let mut args = std::env::args().skip(1);
    let dir = std::path::PathBuf::from(args.next().expect("DIR"));
    let taps: usize = args.next().map_or(100, |t| t.parse().unwrap());
    let read = |n: &str| std::fs::read(dir.join(n)).unwrap();
    let main = read("FirstFrame-Os");
    let libs = [
        "UIKit.dylib",
        "Foundation.dylib",
        "libobjc.dylib",
        "libSystem.dylib",
    ]
    .map(read);
    let image =
        Image::link_main(&main, &libs.iter().map(Vec::as_slice).collect::<Vec<_>>()).unwrap();
    let interp = Prepared::new(&image, false).unwrap();
    let budget = 1_000_000_000;
    let (startup, _, startup_seconds) = run(&interp, 0);
    let (outcome, frames, seconds) = run(&interp, taps);
    let used = |o: &Outcome| budget - o.cpu.remaining;
    let per_frame = (used(&outcome) - used(&startup)) as f64 / taps as f64;
    println!(
        "{{\"engine\":\"interp\",\"taps\":{taps},\"startup_instructions\":{},\"instructions_per_frame\":{per_frame:.0},\"startup_ms\":{:.2},\"ms_per_frame\":{:.3}}}",
        used(&startup),
        startup_seconds * 1e3,
        (seconds - startup_seconds) * 1e3 / taps as f64
    );
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    for native in [false, true] {
        let jit = Prepared::new_with_dispatch(&image, native).unwrap();
        let (jit_startup, _, jit_startup_seconds) = run(&jit, 0);
        let (jit_outcome, jit_frames, jit_seconds) = run(&jit, taps);
        assert_eq!(
            jit_outcome, outcome,
            "JIT outcome differs (dispatch {native})"
        );
        assert_eq!(jit_frames, frames, "JIT frames differ (dispatch {native})");
        assert_eq!(jit_startup, startup);
        println!(
            "{{\"engine\":\"jit\",\"native_dispatch\":{native},\"taps\":{taps},\"equal_outcome_and_frames\":true,\"startup_ms\":{:.2},\"ms_per_frame\":{:.3}}}",
            jit_startup_seconds * 1e3,
            (jit_seconds - jit_startup_seconds) * 1e3 / taps as f64
        );
    }
}
