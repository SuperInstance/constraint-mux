//! Integration tests for constraint-mux consonance & protocol

use constraint_mux::consonance::*;
use constraint_mux::protocol::*;

#[test]
fn test_lattice_point_ratio() {
    assert!((LatticePoint::new(1, 0, 0).ratio() - 2.0).abs() < 1e-10);
    assert!((LatticePoint::new(-1, 1, 0).ratio() - 1.5).abs() < 1e-10);
    assert!((LatticePoint::new(-2, 0, 1).ratio() - 1.25).abs() < 1e-10);
}

#[test]
fn test_lattice_point_zero() {
    let z = LatticePoint::zero();
    assert!((z.ratio() - 1.0).abs() < 1e-10);
}

#[test]
fn test_consonance_heatmap_symmetry() {
    let mut hm = ConsonanceHeatmap::new(10, 100.0, 1000.0);
    hm.record_pair(440.0, 660.0);
    for i in 0..10 {
        for j in 0..10 {
            assert!((hm.heatmap[i][j] - hm.heatmap[j][i]).abs() < 1e-6);
        }
    }
}

#[test]
fn test_consonance_message_roundtrip_binary() {
    let msg = ConsonanceMessage::new(12345, 440.0, LatticePoint::zero(), 1.0, 0);
    let encoded = msg.encode();
    let decoded = ConsonanceMessage::decode(&encoded).unwrap();
    assert_eq!(msg, decoded);
}

#[test]
fn test_consonance_message_framed() {
    let msg = ConsonanceMessage::new(99, 261.63, LatticePoint::new(-2, 0, 1), 0.85, 2);
    let framed = msg.encode_framed();
    let len = u32::from_be_bytes([framed[0], framed[1], framed[2], framed[3]]) as usize;
    assert_eq!(len + 4, framed.len());
}

#[test]
fn test_mux_message_json_roundtrip() {
    let msgs = vec![
        MuxMessage::Hello,
        MuxMessage::ConsonanceSubscribe { min_score: Some(0.5) },
        MuxMessage::HeatmapUpdate { data: vec![vec![0.1], vec![0.2]] },
    ];
    for msg in msgs {
        let json = serde_json::to_vec(&msg).unwrap();
        let decoded: MuxMessage = serde_json::from_slice(&json).unwrap();
        assert_eq!(msg, decoded);
    }
}

#[test]
fn test_frequency_to_note_a4() {
    let (note, octave) = frequency_to_note(440.0);
    assert_eq!(note, "A");
    assert_eq!(octave, 4);
}

#[test]
fn test_consonance_perfect_intervals() {
    let unison = consonance_score(440.0, 440.0);
    let fifth = consonance_score(440.0, 660.0);
    // Unison ratio=1 → distance 0 → score=1.0
    assert!(unison > 0.5, "unison should score high: {unison}");
    // Fifth should be consonant (positive score)
    assert!(fifth > 0.0, "fifth should be consonant: {fifth}");
}
