//! UI-profile host inputs. The SDK-compiled app itself is exercised by
//! scripts/firstframe-driver.py and examples/firstframe_bench.rs, since an
//! SDK-linked binary is never committed.
use sable::ui::{Event, parse_event, scene_delegate_from_bplist};

const PLIST: &[u8] = include_bytes!("../fixtures/apps/FirstFrame/Info.bplist");

#[test]
fn scene_manifest_names_the_scene_delegate() {
    assert_eq!(scene_delegate_from_bplist(PLIST).unwrap(), "SceneDelegate");
}

#[test]
fn malformed_or_unbounded_plists_reject() {
    let mut cases: Vec<Vec<u8>> = vec![
        Vec::new(),
        PLIST[..PLIST.len() - 1].to_vec(),
        PLIST.iter().map(|_| 0).collect(),
        b"<?xml version=\"1.0\"?><plist/>".to_vec(),
    ];
    // Corrupt the trailer's object count, top object and offset-table position.
    for at in [PLIST.len() - 25, PLIST.len() - 9, PLIST.len() - 1] {
        let mut bad = PLIST.to_vec();
        bad[at] ^= 0xff;
        cases.push(bad);
    }
    // Rename the key so the scene path is missing.
    let mut renamed = PLIST.to_vec();
    let key = b"UISceneDelegateClassName";
    let at = renamed.windows(key.len()).position(|w| w == key).unwrap();
    renamed[at] = b'X';
    cases.push(renamed);
    // A class name that is not an identifier.
    let mut spaced = PLIST.to_vec();
    let name = b"SceneDelegate";
    let at = spaced.windows(name.len()).position(|w| w == name).unwrap();
    spaced[at + 5] = b' ';
    cases.push(spaced);
    for (n, bad) in cases.iter().enumerate() {
        assert!(scene_delegate_from_bplist(bad).is_err(), "case {n}");
    }
}

#[test]
fn input_events_are_strict_and_fixed_size() {
    let event = parse_event(r#"{"x":12.5,"phase":"began","y":800,"type":"touch"}"#).unwrap();
    assert_eq!(event, Event::TouchBegan { x: 12.5, y: 800.0 });
    let record = event.record(7);
    assert_eq!(record.len(), 32);
    assert_eq!(&record[..8], &[1, 0, 0, 0, 7, 0, 0, 0]);
    assert_eq!(f64::from_le_bytes(record[8..16].try_into().unwrap()), 12.5);
    assert!(parse_event(r#"{"type":"touch","phase":"began","x":12.5,"y":844}"#).is_err());
}
