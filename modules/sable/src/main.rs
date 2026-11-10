use sable::{Result, engine::run, macho::Image};
use std::{io::Write, time::Instant};

fn main() {
    match execute() {
        Ok(code) => std::process::exit(code as i32),
        Err(e) => {
            eprintln!("sable: {e}");
            std::process::exit(125);
        }
    }
}

fn execute() -> Result<u8> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 3 {
        return Err("usage: sable inspect FILE | sable run FILE [interp|jit] [BUDGET] | sable bench FILE [PAIRS]".into());
    }
    let image = Image::parse(&std::fs::read(&args[2]).map_err(|e| e.to_string())?)?;
    match args[1].as_str() {
        "inspect" => {
            println!(
                "ARM64 Mach-O; platform={}; entry={:#x}; text={:#x}+{}; segments={}",
                image.platform,
                image.entry,
                image.text.0,
                image.text.1,
                image.segments.len()
            );
            Ok(0)
        }
        "run" => {
            let jit = match args.get(3).map(String::as_str).unwrap_or("interp") {
                "jit" => true,
                "interp" => false,
                _ => return Err("engine must be interp or jit".into()),
            };
            let budget = args
                .get(4)
                .map(|s| s.parse::<u64>())
                .transpose()
                .map_err(|e| e.to_string())?
                .unwrap_or(100_000_000);
            let result = run(&image, budget, jit)?;
            std::io::stdout()
                .write_all(&result.stdout)
                .map_err(|e| e.to_string())?;
            std::io::stderr()
                .write_all(&result.stderr)
                .map_err(|e| e.to_string())?;
            eprintln!(
                "instructions={} syscalls={} exit={} x9={}",
                budget - result.cpu.remaining,
                result.syscalls,
                result.exit,
                result.cpu.x[9]
            );
            Ok(result.exit)
        }
        "bench" => {
            let pairs: usize = args
                .get(3)
                .map(|s| s.parse())
                .transpose()
                .map_err(|e: std::num::ParseIntError| e.to_string())?
                .unwrap_or(9);
            if !(3..=101).contains(&pairs) {
                return Err("pairs must be 3..101".into());
            }
            let mut timings = [Vec::new(), Vec::new()];
            // Warm both engines, then alternate order. Translation cost is included
            // on every run; no hidden persistent JIT cache is given to one engine.
            let reference = run(&image, 500_000_000, false)?;
            if run(&image, 500_000_000, true)? != reference {
                return Err("engine state/output mismatch during warmup".into());
            }
            for pair in 0..pairs {
                for i in 0..2 {
                    let engine = (i + pair) % 2;
                    let started = Instant::now();
                    let result = run(&image, 500_000_000, engine == 1)?;
                    let elapsed = started.elapsed().as_secs_f64();
                    if result != reference {
                        return Err("engine state/output mismatch during benchmark".into());
                    }
                    timings[engine].push(elapsed);
                }
            }
            let median = |values: &[f64]| {
                let mut v = values.to_vec();
                v.sort_by(f64::total_cmp);
                v[v.len() / 2]
            };
            println!(
                "{{\"benchmark\":\"arm64_arithmetic_translation\",\"pairs\":{pairs},\"instructions\":{},\"checksum_x9\":{},\"equal_full_state\":true,\"translation_included\":true,\"interp_seconds\":{:?},\"jit_seconds\":{:?},\"median_speedup\":{}}}",
                500_000_000 - reference.cpu.remaining,
                reference.cpu.x[9],
                timings[0],
                timings[1],
                median(&timings[0]) / median(&timings[1])
            );
            Ok(0)
        }
        _ => Err("unknown command".into()),
    }
}
