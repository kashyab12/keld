//! Host services for Sable's fixed UI profile: launch information, presenting a
//! guest-rendered BGRA8 frame and delivering one external input event.
//!
//! The guest produces every pixel. The host only copies presented bytes out
//! unchanged and parses strict input; it never creates or alters pixel values.
use crate::Result;
use std::io::{BufRead, Write};
use std::path::PathBuf;

/// Fixed profile: 390x844 points at scale 1, BGRA8, tightly packed rows.
pub const WIDTH: u64 = 390;
pub const HEIGHT: u64 = 844;
pub const STRIDE: u64 = WIDTH * 4;

pub const LAUNCH_INFO: u32 = 0xd400_0001 | (0x5b50 << 5);
pub const PRESENT: u32 = 0xd400_0001 | (0x5b51 << 5);
pub const WAIT_EVENT: u32 = 0xd400_0001 | (0x5b52 << 5);
pub fn is_service(word: u32) -> bool {
    matches!(word, LAUNCH_INFO | PRESENT | WAIT_EVENT)
}

/// Export addresses of the checked `svc; ret` stubs in the UIKit provider.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Runtime {
    pub launch_info: u64,
    pub present: u64,
    pub wait_event: u64,
}
impl Runtime {
    pub(crate) fn origin(&self, pc: u64, word: u32) -> bool {
        matches!((word, pc), (LAUNCH_INFO, p) if p == self.launch_info)
            || matches!((word, pc), (PRESENT, p) if p == self.present)
            || matches!((word, pc), (WAIT_EVENT, p) if p == self.wait_event)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Event {
    TouchBegan { x: f64, y: f64 },
    TouchEnded { x: f64, y: f64 },
    Quit,
}
impl Event {
    /// The fixed 32-byte guest record: kind, sequence, x, y, reserved.
    pub fn record(&self, sequence: u32) -> [u8; 32] {
        let (kind, x, y) = match *self {
            Event::TouchBegan { x, y } => (1u32, x, y),
            Event::TouchEnded { x, y } => (2, x, y),
            Event::Quit => (3, 0.0, 0.0),
        };
        let mut out = [0u8; 32];
        out[..4].copy_from_slice(&kind.to_le_bytes());
        out[4..8].copy_from_slice(&sequence.to_le_bytes());
        out[8..16].copy_from_slice(&x.to_bits().to_le_bytes());
        out[16..24].copy_from_slice(&y.to_bits().to_le_bytes());
        out
    }
    pub fn kind(&self) -> u64 {
        match self {
            Event::TouchBegan { .. } => 1,
            Event::TouchEnded { .. } => 2,
            Event::Quit => 3,
        }
    }
}

/// What the engine needs from the embedding process.
pub trait Host {
    /// UISceneDelegateClassName from the app's scene manifest.
    fn scene_delegate(&mut self) -> Result<String>;
    /// Called with exactly HEIGHT * STRIDE guest bytes.
    fn present(&mut self, pixels: &[u8]) -> Result<()>;
    fn wait_event(&mut self) -> Result<Event>;
}

/// Strict JSON-lines event: `{"type":"touch","phase":"began"|"ended","x":N,"y":N}`
/// or `{"type":"quit"}`. Keys may appear in any order; nothing else is accepted.
pub fn parse_event(line: &str) -> Result<Event> {
    let bad = || format!("malformed input event: {line:?}");
    let body = line
        .trim_end_matches(['\r', '\n'])
        .strip_prefix('{')
        .and_then(|b| b.strip_suffix('}'))
        .ok_or_else(bad)?;
    if line.len() > 256 || body.contains(['{', '}', '[', ']', '\\']) {
        return Err(bad());
    }
    let mut fields = std::collections::BTreeMap::new();
    for pair in body.split(',') {
        let (key, value) = pair.split_once(':').ok_or_else(bad)?;
        let key = key
            .trim()
            .strip_prefix('"')
            .and_then(|k| k.strip_suffix('"'))
            .ok_or_else(bad)?;
        if fields.insert(key, value.trim()).is_some() {
            return Err(bad());
        }
    }
    fn string(v: &str) -> Option<&str> {
        v.strip_prefix('"')
            .and_then(|s| s.strip_suffix('"'))
            .filter(|s| !s.contains('"'))
    }
    let number = |v: &str, limit: f64| -> Option<f64> {
        if v.is_empty()
            || !v
                .bytes()
                .all(|b| b.is_ascii_digit() || matches!(b, b'.' | b'-' | b'e' | b'E' | b'+'))
        {
            return None;
        }
        v.parse::<f64>()
            .ok()
            .filter(|n| n.is_finite() && *n >= 0.0 && *n < limit)
    };
    match fields.get("type").and_then(|v| string(v)) {
        Some("quit") if fields.len() == 1 => Ok(Event::Quit),
        Some("touch") if fields.len() == 4 => {
            let x = fields
                .get("x")
                .and_then(|v| number(v, WIDTH as f64))
                .ok_or_else(bad)?;
            let y = fields
                .get("y")
                .and_then(|v| number(v, HEIGHT as f64))
                .ok_or_else(bad)?;
            match fields.get("phase").and_then(|v| string(v)) {
                Some("began") => Ok(Event::TouchBegan { x, y }),
                Some("ended") => Ok(Event::TouchEnded { x, y }),
                _ => Err(bad()),
            }
        }
        _ => Err(bad()),
    }
}

/// Bounded binary-plist reader for exactly one path:
/// UIApplicationSceneManifest.UISceneConfigurations.UIWindowSceneSessionRoleApplication[0]
/// .UISceneDelegateClassName. It understands dicts, arrays, ASCII strings and bools.
pub fn scene_delegate_from_bplist(data: &[u8]) -> Result<String> {
    let bad = |why: &str| format!("invalid Info.plist: {why}");
    if data.len() > 64 * 1024 || data.len() < 40 || &data[..8] != b"bplist00" {
        return Err(bad("not a bounded bplist00"));
    }
    let trailer = &data[data.len() - 32..];
    let offset_size = trailer[6] as usize;
    let ref_size = trailer[7] as usize;
    let be = |bytes: &[u8]| bytes.iter().fold(0u64, |v, &b| (v << 8) | u64::from(b));
    let objects = be(&trailer[8..16]) as usize;
    let top = be(&trailer[16..24]) as usize;
    let table = be(&trailer[24..32]) as usize;
    if !(1..=8).contains(&offset_size)
        || !(1..=8).contains(&ref_size)
        || objects == 0
        || objects > 4096
        || top >= objects
        || table
            .checked_add(objects * offset_size)
            .is_none_or(|end| end > data.len() - 32)
    {
        return Err(bad("trailer"));
    }
    let offset = |n: usize| -> Result<usize> {
        let at = table + n * offset_size;
        let o = be(&data[at..at + offset_size]) as usize;
        if o < 8 || o >= table {
            return Err(bad("object offset"));
        }
        Ok(o)
    };
    // (marker, count, payload start) with the extended count form.
    let header = |n: usize| -> Result<(u8, usize, usize)> {
        if n >= objects {
            return Err(bad("object reference"));
        }
        let o = offset(n)?;
        let marker = data[o];
        let mut count = (marker & 15) as usize;
        let mut start = o + 1;
        if count == 15 && matches!(marker >> 4, 0x5 | 0xa | 0xd) {
            let int = *data.get(start).ok_or_else(|| bad("count"))?;
            let width = 1usize << (int & 15);
            if int >> 4 != 1 || width > 8 || start + 1 + width > table {
                return Err(bad("count"));
            }
            count = be(&data[start + 1..start + 1 + width]) as usize;
            start += 1 + width;
        }
        if count > 4096 {
            return Err(bad("count"));
        }
        Ok((marker, count, start))
    };
    let reference = |at: usize| -> Result<usize> {
        if at + ref_size > table {
            return Err(bad("reference"));
        }
        Ok(be(&data[at..at + ref_size]) as usize)
    };
    let ascii = |n: usize| -> Result<String> {
        let (marker, count, start) = header(n)?;
        if marker >> 4 != 0x5 || start + count > table || count > 255 {
            return Err(bad("expected an ASCII string"));
        }
        let bytes = &data[start..start + count];
        if !bytes.iter().all(|b| (0x20..0x7f).contains(b)) {
            return Err(bad("non-ASCII string"));
        }
        Ok(String::from_utf8(bytes.to_vec()).unwrap())
    };
    let lookup = |dict: usize, key: &str| -> Result<usize> {
        let (marker, count, start) = header(dict)?;
        if marker >> 4 != 0xd {
            return Err(bad("expected a dictionary"));
        }
        for i in 0..count {
            if ascii(reference(start + i * ref_size)?).ok().as_deref() == Some(key) {
                return reference(start + (count + i) * ref_size);
            }
        }
        Err(bad(&format!("missing key {key}")))
    };
    let manifest = lookup(top, "UIApplicationSceneManifest")?;
    let configurations = lookup(manifest, "UISceneConfigurations")?;
    let role = lookup(configurations, "UIWindowSceneSessionRoleApplication")?;
    let (marker, count, start) = header(role)?;
    if marker >> 4 != 0xa || count == 0 {
        return Err(bad("expected a non-empty scene configuration array"));
    }
    let name = ascii(lookup(reference(start)?, "UISceneDelegateClassName")?)?;
    if name.is_empty()
        || name.len() > 63
        || !name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
    {
        return Err(bad("UISceneDelegateClassName is not a class identifier"));
    }
    Ok(name)
}

/// SHA-256 (FIPS 180-4), used to fingerprint presented frames.
pub fn sha256(data: &[u8]) -> [u8; 32] {
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
        0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
        0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
        0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
        0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
        0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
        0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
        0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
        0xc67178f2,
    ];
    let mut h: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
        0x5be0cd19,
    ];
    let mut message = data.to_vec();
    message.push(0x80);
    while message.len() % 64 != 56 {
        message.push(0);
    }
    message.extend_from_slice(&((data.len() as u64) * 8).to_be_bytes());
    for block in message.chunks(64) {
        let mut w = [0u32; 64];
        for (i, word) in block.chunks(4).enumerate() {
            w[i] = u32::from_be_bytes(word.try_into().unwrap());
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }
        let mut v = h;
        for i in 0..64 {
            let s1 = v[4].rotate_right(6) ^ v[4].rotate_right(11) ^ v[4].rotate_right(25);
            let ch = (v[4] & v[5]) ^ (!v[4] & v[6]);
            let t1 = v[7]
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(K[i])
                .wrapping_add(w[i]);
            let s0 = v[0].rotate_right(2) ^ v[0].rotate_right(13) ^ v[0].rotate_right(22);
            let maj = (v[0] & v[1]) ^ (v[0] & v[2]) ^ (v[1] & v[2]);
            let t2 = s0.wrapping_add(maj);
            v = [
                t1.wrapping_add(t2),
                v[0],
                v[1],
                v[2],
                v[3].wrapping_add(t1),
                v[4],
                v[5],
                v[6],
            ];
        }
        for (a, b) in h.iter_mut().zip(v) {
            *a = a.wrapping_add(b);
        }
    }
    let mut out = [0u8; 32];
    for (i, word) in h.iter().enumerate() {
        out[i * 4..i * 4 + 4].copy_from_slice(&word.to_be_bytes());
    }
    out
}

pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// CLI host: scene name from a binary Info.plist, frames written as
/// `frames/NNNN.rgba` then `NNNN.sha256` (the sha file appears last, so a
/// watcher never sees a partial frame), and events read from `input`.
pub struct FileHost {
    pub scene: String,
    pub frames: PathBuf,
    pub input: Box<dyn BufRead>,
    pub presented: u32,
    pub events: Vec<String>,
}
impl Host for FileHost {
    fn scene_delegate(&mut self) -> Result<String> {
        Ok(self.scene.clone())
    }
    fn present(&mut self, pixels: &[u8]) -> Result<()> {
        if self.presented >= 9999 {
            return Err("frame count cap reached".into());
        }
        self.presented += 1;
        let stem = self.frames.join(format!("{:04}", self.presented));
        let digest = hex(&sha256(pixels));
        std::fs::write(stem.with_extension("rgba"), pixels).map_err(|e| e.to_string())?;
        let partial = stem.with_extension("sha256.partial");
        std::fs::write(&partial, format!("{digest}\n")).map_err(|e| e.to_string())?;
        std::fs::rename(&partial, stem.with_extension("sha256")).map_err(|e| e.to_string())?;
        Ok(())
    }
    fn wait_event(&mut self) -> Result<Event> {
        let mut line = String::new();
        let mut input: &mut dyn BufRead = &mut *self.input;
        if std::io::Read::take(&mut input, 257)
            .read_line(&mut line)
            .map_err(|e| e.to_string())?
            == 0
        {
            return Err("input closed before quit".into());
        }
        let event = parse_event(&line)?;
        self.events.push(format!(
            "{{\"seq\":{},\"next_frame\":{},\"event\":{}}}",
            self.events.len() + 1,
            self.presented + 1,
            line.trim_end()
        ));
        let mut log = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(self.frames.join("events.jsonl"))
            .map_err(|e| e.to_string())?;
        writeln!(log, "{}", self.events.last().unwrap()).map_err(|e| e.to_string())?;
        Ok(event)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sha256_known_vectors() {
        assert_eq!(
            hex(&sha256(b"")),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            hex(&sha256(b"abc")),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }
    #[test]
    fn events_are_strict() {
        assert_eq!(
            parse_event(r#"{"type":"touch","phase":"began","x":12.5,"y":3}"#).unwrap(),
            Event::TouchBegan { x: 12.5, y: 3.0 }
        );
        assert_eq!(parse_event("{\"type\":\"quit\"}\n").unwrap(), Event::Quit);
        for bad in [
            "",
            "{}",
            r#"{"type":"quit","x":1}"#,
            r#"{"type":"touch","phase":"began","x":390,"y":3}"#,
            r#"{"type":"touch","phase":"began","x":-1,"y":3}"#,
            r#"{"type":"touch","phase":"began","x":"1","y":3}"#,
            r#"{"type":"touch","phase":"moved","x":1,"y":3}"#,
            r#"{"type":"touch","phase":"began","x":NaN,"y":3}"#,
            r#"{"type":"touch","phase":"began","x":1,"x":2,"y":3}"#,
            r#"{"type":"touch","phase":"began","x":1e999,"y":3}"#,
            r#"{"type":"quit"} trailing"#,
            r#"{"type":"touch","phase":"began","x":[1],"y":3}"#,
        ] {
            assert!(parse_event(bad).is_err(), "{bad}");
        }
    }
}
