//! Read `address word` hex pairs on stdin; print every word Sable's decoder rejects.
use std::io::BufRead;

fn main() {
    for line in std::io::stdin().lock().lines() {
        let line = line.unwrap();
        let mut parts = line.split_whitespace();
        let (Some(pc), Some(word)) = (parts.next(), parts.next()) else {
            continue;
        };
        let pc = u64::from_str_radix(pc, 16).unwrap();
        let word = u32::from_str_radix(word, 16).unwrap();
        if let Err(e) = sable::isa::decode(word, pc) {
            println!("{word:08x} {e}");
        }
    }
}
