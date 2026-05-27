//! Consonance scoring — pure Rust port of constraint-audio consonance theory.
//!
//! Models consonance based on the harmonic lattice (2^a × 3^b × 5^c).
//! Each frequency is decomposed into lattice coordinates, and the consonance
//! of frequency pairs is computed from their lattice distance.

/// Lattice point in the 3-limit (2,3,5) harmonic space.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct LatticePoint {
    pub a: i8, // power of 2
    pub b: i8, // power of 3
    pub c: i8, // power of 5
}

impl LatticePoint {
    pub fn new(a: i8, b: i8, c: i8) -> Self {
        Self { a, b, c }
    }

    /// Zero lattice point (unison).
    pub fn zero() -> Self {
        Self { a: 0, b: 0, c: 0 }
    }

    /// Compute the ratio as a float: 2^a × 3^b × 5^c
    pub fn ratio(&self) -> f64 {
        2.0_f64.powi(self.a as i32) * 3.0_f64.powi(self.b as i32) * 5.0_f64.powi(self.c as i32)
    }

    /// Euclidean distance on the lattice (simple metric).
    pub fn distance(&self, other: &LatticePoint) -> f64 {
        let da = (self.a as f64 - other.a as f64).powi(2);
        let db = (self.b as f64 - other.b as f64).powi(2);
        let dc = (self.c as f64 - other.c as f64).powi(2);
        (da + db + dc).sqrt()
    }
}

/// Approximate a frequency ratio as a lattice point using Farey-like enumeration
/// over a bounded region.
pub fn ratio_to_lattice(ratio: f64) -> LatticePoint {
    // Pre-computed table of common just intervals within 2 octaves
    // (ratio_numerator/denominator, a, b, c) where ratio = 2^a * 3^b * 5^c
    let candidates: &[(f64, i8, i8, i8)] = &[
        (1.0, 0, 0, 0),           // unison
        (16.0 / 15.0, -4, 1, -1), // minor second
        (9.0 / 8.0, -3, 2, 0),   // major second
        (6.0 / 5.0, 1, 1, -1),   // minor third
        (5.0 / 4.0, -2, 0, 1),   // major third
        (4.0 / 3.0, 2, -1, 0),   // perfect fourth
        (45.0 / 32.0, -5, 2, 1), // tritone
        (3.0 / 2.0, -1, 1, 0),   // perfect fifth
        (8.0 / 5.0, 3, 0, -1),   // minor sixth
        (5.0 / 3.0, -1, -1, 1),  // major sixth
        (9.0 / 5.0, 0, 2, -1),   // minor seventh
        (15.0 / 8.0, -3, 1, 1),  // major seventh
        (2.0, 1, 0, 0),          // octave
    ];

    // Normalize ratio to [1, 2] by shifting octaves
    let mut r = ratio;
    while r > 2.0 {
        r /= 2.0;
    }
    while r < 1.0 && r > 0.0 {
        r *= 2.0;
    }

    let mut best = LatticePoint::zero();
    let mut best_error = f64::MAX;

    for &(cand_ratio, a, b, c) in candidates {
        let error = (r - cand_ratio).abs();
        if error < best_error {
            best_error = error;
            best = LatticePoint::new(a, b, c);
        }
    }

    // Special case: normalized ratio very close to 1.0 means unison/octave
    // This is correct — the lattice point (1,0,0) means "one octave up"
    // but after normalization, 2.0 → 1.0 → unison point, which is fine
    // for consonance scoring.
    best
}

/// Compute consonance score for a pair of frequencies.
///
/// Returns a value in [0.0, 1.0] where 1.0 is maximally consonant (unison/octave)
/// and 0.0 is maximally dissonant.
pub fn consonance_score(freq1: f64, freq2: f64) -> f32 {
    if freq1 <= 0.0 || freq2 <= 0.0 {
        return 0.0;
    }

    let ratio = freq1 / freq2;
    let lp = ratio_to_lattice(ratio);
    let distance = lp.distance(&LatticePoint::zero());

    // Map lattice distance to [0, 1] — lower distance = more consonant
    // Using exponential decay: score = exp(-distance * k)
    let k = 0.5;
    (-distance * k).exp() as f32
}

/// Classify a frequency as a musical note name + octave.
pub fn frequency_to_note(freq: f64) -> (String, u32) {
    if freq <= 0.0 {
        return ("?".to_string(), 0);
    }

    let note_names = ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"];
    let semitones = 12.0 * (freq / 440.0).log2();
    let midi = (semitones + 69.0).round() as i32;
    let note_index = ((midi % 12 + 12) % 12) as usize;
    let octave = (midi / 12 - 1) as u32;
    (note_names[note_index].to_string(), octave)
}

/// A consonance event: a detected frequency with its analysis.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ConsonanceEvent {
    /// Timestamp in nanoseconds since epoch.
    pub timestamp_ns: u64,
    /// Detected frequency in Hz.
    pub frequency: f64,
    /// Lattice coordinates.
    pub lattice: LatticePoint,
    /// Consonance score relative to the previous event (or 1.0 if first).
    pub consonance: f32,
    /// Voice/channel identifier.
    pub voice_id: u8,
}

/// A rolling consonance heatmap tracker.
/// Tracks frequency bins and their pairwise consonance over time.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ConsonanceHeatmap {
    /// Number of frequency bins.
    pub bins: usize,
    /// Frequency range (min, max) in Hz.
    pub freq_range: (f64, f64),
    /// 2D heatmap: heatmap[i][j] = accumulated consonance between bins i and j.
    pub heatmap: Vec<Vec<f32>>,
    /// Hit count for normalization.
    pub counts: Vec<Vec<u32>>,
}

impl ConsonanceHeatmap {
    pub fn new(bins: usize, freq_min: f64, freq_max: f64) -> Self {
        let heatmap = vec![vec![0.0f32; bins]; bins];
        let counts = vec![vec![0u32; bins]; bins];
        Self {
            bins,
            freq_range: (freq_min, freq_max),
            heatmap,
            counts,
        }
    }

    /// Map a frequency to a bin index.
    pub fn freq_to_bin(&self, freq: f64) -> Option<usize> {
        if freq < self.freq_range.0 || freq > self.freq_range.1 {
            return None;
        }
        let ratio = (freq - self.freq_range.0) / (self.freq_range.1 - self.freq_range.0);
        Some(((ratio * self.bins as f64) as usize).min(self.bins - 1))
    }

    /// Record a pair of frequencies into the heatmap.
    pub fn record_pair(&mut self, freq1: f64, freq2: f64) {
        let bin1 = match self.freq_to_bin(freq1) {
            Some(b) => b,
            None => return,
        };
        let bin2 = match self.freq_to_bin(freq2) {
            Some(b) => b,
            None => return,
        };
        let score = consonance_score(freq1, freq2);
        self.heatmap[bin1][bin2] += score;
        self.counts[bin1][bin2] += 1;
        if bin1 != bin2 {
            self.heatmap[bin2][bin1] += score;
            self.counts[bin2][bin1] += 1;
        }
    }

    /// Get normalized heatmap values [0, 1].
    pub fn normalized(&self) -> Vec<Vec<f32>> {
        let max_val = self
            .heatmap
            .iter()
            .flat_map(|row| row.iter())
            .copied()
            .fold(0.0f32, f32::max);
        if max_val == 0.0 {
            return vec![vec![0.0f32; self.bins]; self.bins];
        }
        self.heatmap
            .iter()
            .map(|row| row.iter().map(|&v| v / max_val).collect())
            .collect()
    }

    /// Decay all values by a factor (for temporal smoothing).
    pub fn decay(&mut self, factor: f32) {
        for row in &mut self.heatmap {
            for val in row.iter_mut() {
                *val *= factor;
            }
        }
        for row in &mut self.counts {
            for val in row.iter_mut() {
                *val = (*val as f32 * factor) as u32;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lattice_ratio() {
        assert!((LatticePoint::new(1, 0, 0).ratio() - 2.0).abs() < 1e-10);
        assert!((LatticePoint::new(-1, 1, 0).ratio() - 1.5).abs() < 1e-10);
        assert!((LatticePoint::new(-2, 0, 1).ratio() - 1.25).abs() < 1e-10);
    }

    #[test]
    fn test_lattice_distance() {
        let origin = LatticePoint::zero();
        assert_eq!(origin.distance(&origin), 0.0);
        assert!((origin.distance(&LatticePoint::new(1, 0, 0)) - 1.0).abs() < 1e-10);
    }

    #[test]
    fn test_ratio_to_lattice_perfect_fifth() {
        // 3/2 = perfect fifth
        let lp = ratio_to_lattice(1.5);
        assert_eq!(lp, LatticePoint::new(-1, 1, 0));
    }

    #[test]
    fn test_ratio_to_lattice_octave() {
        // 2.0 stays as 2.0 (not > 2.0, so no normalization), matches octave candidate
        let lp = ratio_to_lattice(2.0);
        assert_eq!(lp, LatticePoint::new(1, 0, 0));
    }

    #[test]
    fn test_ratio_to_lattice_major_third() {
        let lp = ratio_to_lattice(1.25);
        assert_eq!(lp, LatticePoint::new(-2, 0, 1));
    }

    #[test]
    fn test_consonance_unison() {
        // Same frequency → maximum consonance
        let score = consonance_score(440.0, 440.0);
        assert!(score > 0.99);
    }

    #[test]
    fn test_consonance_octave() {
        // Octave → very high consonance
        let score = consonance_score(440.0, 880.0);
        assert!(score > 0.9);
    }

    #[test]
    fn test_consonance_tritone() {
        // Tritone → lower consonance than fifth
        let tritone = consonance_score(440.0, 622.25);
        let fifth = consonance_score(440.0, 660.0);
        assert!(fifth > tritone);
    }

    #[test]
    fn test_frequency_to_note() {
        let (note, octave) = frequency_to_note(440.0);
        assert_eq!(note, "A");
        assert_eq!(octave, 4);

        let (note, octave) = frequency_to_note(261.63);
        assert_eq!(note, "C");
        assert_eq!(octave, 4);
    }

    #[test]
    fn test_heatmap() {
        let mut hm = ConsonanceHeatmap::new(10, 100.0, 1000.0);
        hm.record_pair(440.0, 660.0); // perfect fifth
        hm.record_pair(440.0, 440.0); // unison

        let norm = hm.normalized();
        // Unison bin should have highest consonance
        let max = norm.iter().flat_map(|r| r.iter()).copied().fold(0.0f32, f32::max);
        assert!((max - 1.0).abs() < 0.01);
    }

    #[test]
    fn test_lattice_zero() {
        let z = LatticePoint::zero();
        assert_eq!(z.a, 0);
        assert_eq!(z.b, 0);
        assert_eq!(z.c, 0);
        assert!((z.ratio() - 1.0).abs() < 1e-10);
    }

    #[test]
    fn test_lattice_copy_eq() {
        let a = LatticePoint::new(1, 2, 3);
        let b = a;
        assert_eq!(a, b);
    }

    #[test]
    fn test_consonance_zero_frequency() {
        assert_eq!(consonance_score(0.0, 440.0), 0.0);
        assert_eq!(consonance_score(440.0, 0.0), 0.0);
        assert_eq!(consonance_score(0.0, 0.0), 0.0);
    }

    #[test]
    fn test_consonance_negative_frequency() {
        assert_eq!(consonance_score(-100.0, 440.0), 0.0);
        assert_eq!(consonance_score(440.0, -100.0), 0.0);
    }

    #[test]
    fn test_consonance_perfect_fourth() {
        // 4/3 ratio — should be reasonably consonant (score > 0.3)
        let score = consonance_score(440.0, 440.0 * 4.0 / 3.0);
        assert!(score > 0.3, "perfect fourth should have some consonance: {score}");
    }

    #[test]
    fn test_ratio_to_lattice_large_ratio() {
        // Ratio 4.0: while > 2.0 loop, 4.0→2.0 (not > 2.0, stops), matches octave candidate
        let lp = ratio_to_lattice(4.0);
        assert_eq!(lp, LatticePoint::new(1, 0, 0)); // matches octave
    }

    #[test]
    fn test_ratio_to_lattice_small_ratio() {
        // Ratio < 1 should normalize up
        let lp = ratio_to_lattice(0.75); // 3/4 → normalizes to 3/2 (perfect fifth)
        assert_eq!(lp, LatticePoint::new(-1, 1, 0));
    }

    #[test]
    fn test_frequency_to_note_zero() {
        let (note, octave) = frequency_to_note(0.0);
        assert_eq!(note, "?");
        assert_eq!(octave, 0);
    }

    #[test]
    fn test_frequency_to_note_negative() {
        let (note, octave) = frequency_to_note(-100.0);
        assert_eq!(note, "?");
        assert_eq!(octave, 0);
    }

    #[test]
    fn test_frequency_to_note_c3() {
        let (note, octave) = frequency_to_note(130.81); // C3
        assert_eq!(note, "C");
        assert_eq!(octave, 3);
    }

    #[test]
    fn test_consonance_event_serde() {
        let evt = ConsonanceEvent {
            timestamp_ns: 12345,
            frequency: 440.0,
            lattice: LatticePoint::new(1, 2, 3),
            consonance: 0.75,
            voice_id: 5,
        };
        let json = serde_json::to_string(&evt).unwrap();
        let decoded: ConsonanceEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.timestamp_ns, 12345);
        assert!((decoded.frequency - 440.0).abs() < 1e-10);
        assert_eq!(decoded.lattice, LatticePoint::new(1, 2, 3));
        assert!((decoded.consonance - 0.75).abs() < 1e-5);
        assert_eq!(decoded.voice_id, 5);
    }

    #[test]
    fn test_lattice_point_serde() {
        let lp = LatticePoint::new(-3, 2, -1);
        let json = serde_json::to_string(&lp).unwrap();
        let decoded: LatticePoint = serde_json::from_str(&json).unwrap();
        assert_eq!(lp, decoded);
    }

    #[test]
    fn test_heatmap_freq_to_bin_in_range() {
        let hm = ConsonanceHeatmap::new(10, 100.0, 1000.0);
        assert_eq!(hm.freq_to_bin(100.0), Some(0));
        assert_eq!(hm.freq_to_bin(1000.0), Some(9));
        assert_eq!(hm.freq_to_bin(550.0), Some(5));
    }

    #[test]
    fn test_heatmap_freq_to_bin_out_of_range() {
        let hm = ConsonanceHeatmap::new(10, 100.0, 1000.0);
        assert_eq!(hm.freq_to_bin(50.0), None);
        assert_eq!(hm.freq_to_bin(2000.0), None);
        assert_eq!(hm.freq_to_bin(-1.0), None);
    }

    #[test]
    fn test_heatmap_normalized_empty() {
        let hm = ConsonanceHeatmap::new(5, 100.0, 1000.0);
        let norm = hm.normalized();
        for row in &norm {
            for &v in row {
                assert_eq!(v, 0.0f32);
            }
        }
    }

    #[test]
    fn test_heatmap_record_pair_out_of_range() {
        let mut hm = ConsonanceHeatmap::new(5, 100.0, 1000.0);
        // Both out of range — should silently skip
        hm.record_pair(50.0, 2000.0);
        let total: f32 = hm.heatmap.iter().flat_map(|r| r.iter()).sum();
        assert_eq!(total, 0.0);
    }

    #[test]
    fn test_heatmap_symmetry() {
        let mut hm = ConsonanceHeatmap::new(10, 100.0, 1000.0);
        hm.record_pair(200.0, 300.0);
        // Heatmap should be symmetric
        for i in 0..10 {
            for j in 0..10 {
                assert!((hm.heatmap[i][j] - hm.heatmap[j][i]).abs() < 1e-6,
                    "heatmap not symmetric at [{i}][{j}]");
            }
        }
    }

    #[test]
    fn test_lattice_distance_asymmetric() {
        let a = LatticePoint::new(1, 2, 3);
        let b = LatticePoint::new(-1, -2, -3);
        // Distance should be symmetric
        let d1 = a.distance(&b);
        let d2 = b.distance(&a);
        assert!((d1 - d2).abs() < 1e-10);
    }

    #[test]
    fn test_heatmap_decay() {
        let mut hm = ConsonanceHeatmap::new(5, 100.0, 1000.0);
        hm.record_pair(200.0, 300.0);
        // Find a non-zero cell
        let mut found_nonzero = false;
        for i in 0..5 {
            for j in 0..5 {
                if hm.heatmap[i][j] > 0.0 {
                    let before = hm.heatmap[i][j];
                    found_nonzero = true;
                    hm.decay(0.5);
                    assert!(hm.heatmap[i][j] < before, "expected decay at [{i}][{j}]");
                    break;
                }
            }
            if found_nonzero {
                break;
            }
        }
        assert!(found_nonzero, "expected some non-zero heatmap entries after recording");
    }
}
