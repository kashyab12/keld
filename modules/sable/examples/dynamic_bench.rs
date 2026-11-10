//! Worker-level benchmark. Every timed run gets fresh private writable state.
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
fn main() {
    use sable::{
        engine::{Outcome, Prepared},
        macho::Image,
    };
    use std::io::Write;
    use std::time::Instant;
    let args: Vec<_> = std::env::args().collect();
    let rounds: usize = args.get(1).expect("round count").parse().unwrap();
    let pairs: usize = args.get(2).map(|s| s.parse().unwrap()).unwrap_or(31);
    assert!((30..=101).contains(&pairs));
    let root = std::path::Path::new("modules/sable/fixtures/bin/dynamic");
    let main = std::fs::read(root.join(format!("dynamic-{rounds}.macho"))).unwrap();
    let math = std::fs::read(root.join("libsable_math.dylib")).unwrap();
    let helper = std::fs::read(root.join("libsable_helper.dylib")).unwrap();
    let system = std::fs::read(root.join("libsable_system.dylib")).unwrap();
    let libraries = [math.as_slice(), helper.as_slice(), system.as_slice()];
    let setup = Instant::now();
    let image = Image::link(&main, &libraries).unwrap();
    let link_seconds = setup.elapsed().as_secs_f64();
    let setup = Instant::now();
    let rust = Prepared::new_with_dispatch(&image, false).unwrap();
    let rust_prepare_seconds = setup.elapsed().as_secs_f64();
    let setup = Instant::now();
    let native = Prepared::new_with_dispatch(&image, true).unwrap();
    let native_prepare_seconds = setup.elapsed().as_secs_f64();
    let expected = match rounds {
        8 => 0x38afd5babu64,
        2000 => 0xefe1edd5865163fb,
        200000 => 0xa7151818301809c9,
        _ => panic!("unsupported fixture"),
    }
    .to_le_bytes();
    let reference = Prepared::new(&image, false)
        .unwrap()
        .run(10_000_000)
        .unwrap();
    assert_eq!(reference.stdout, expected);
    assert_eq!(reference.exit, 0);
    let execute = |variant: usize| -> Outcome {
        match variant {
            0 | 1 => {
                let image = Image::link(&main, &libraries).unwrap();
                Prepared::new_with_dispatch(&image, variant == 1)
                    .unwrap()
                    .run(10_000_000)
                    .unwrap()
            }
            2 => rust.run(10_000_000).unwrap(),
            _ => native.run(10_000_000).unwrap(),
        }
    };
    for variant in 0..4 {
        for _ in 0..2 {
            assert_eq!(execute(variant), reference);
        }
    }
    let mut journal = args.get(3).map(|p| {
        std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(p)
            .expect("new sample journal")
    });
    let mut samples: [Vec<f64>; 4] = std::array::from_fn(|_| Vec::new());
    let mut order = Vec::new();
    let mut seed = 0x9e3779b97f4a7c15u64;
    for _ in 0..pairs {
        let mut block = [0, 1, 2, 3];
        for i in (1..4).rev() {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            block.swap(i, seed as usize % (i + 1));
        }
        order.push(block);
        for variant in block {
            let at = Instant::now();
            let outcome = execute(variant);
            let elapsed = at.elapsed().as_secs_f64();
            let correct = outcome == reference;
            if let Some(file) = &mut journal {
                writeln!(file,"{{\"block\":{},\"variant\":{variant},\"seconds\":{elapsed},\"full_state_equal\":{correct}}}",order.len()-1).unwrap();
            }
            assert!(correct, "full-state mismatch in variant {variant}");
            samples[variant].push(elapsed);
        }
    }
    println!(
        "{{\"rounds\":{rounds},\"pairs\":{pairs},\"variants\":[\"cold_rust_dispatch\",\"cold_native_dispatch\",\"prepared_rust_dispatch\",\"prepared_native_dispatch\"],\"samples_seconds\":{:?},\"order\":{:?},\"full_state_equal\":true,\"checksum_hex\":\"{:016x}\",\"link_seconds\":{link_seconds},\"rust_prepare_seconds\":{rust_prepare_seconds},\"native_prepare_seconds\":{native_prepare_seconds},\"shared_image_bytes_per_run\":{},\"instructions\":{}}}",
        samples,
        order,
        u64::from_le_bytes(expected),
        reference.memory.shared_image_bytes(),
        10_000_000 - reference.cpu.remaining
    );
}

#[cfg(not(all(target_os = "linux", target_arch = "x86_64")))]
fn main() {
    eprintln!("dynamic_bench requires x86_64 Linux");
    std::process::exit(1);
}
