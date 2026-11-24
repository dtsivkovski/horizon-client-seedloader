//! Soft-compare of generated chunks against real server chunks.

/// Top height (solid or fluid) of each column in a chunk. `None` if empty.
pub type Heights = [Option<i32>; 256];

/// Chunks to compare before deciding.
pub const CHUNKS: usize = 16;
/// Columns within this many blocks count as matching.
const TOLERANCE: i32 = 3;
/// Median match share needed for seed to pass.
const PASS: f32 = 0.6;

/// Share of matching columns. `None` if too few have ground.
pub fn similarity(real: &Heights, generated: &Heights) -> Option<f32> {
    let mut compared = 0;
    let mut matching = 0;
    for (a, b) in real.iter().zip(generated) {
        match (a, b) {
            (None, None) => continue,
            (Some(a), Some(b)) if (a - b).abs() <= TOLERANCE => matching += 1,
            _ => {}
        }
        compared += 1;
    }
    (compared >= 64).then(|| matching as f32 / compared as f32)
}

/// Median score, and whether it passes.
pub fn verdict(scores: &mut [f32]) -> (f32, bool) {
    scores.sort_by(f32::total_cmp);
    let median = scores.get(scores.len() / 2).copied().unwrap_or(0.0);
    (median, median >= PASS)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Heights within tolerance count as matching.
    #[test]
    fn close_heights_match() {
        let real: Heights = std::array::from_fn(|i| Some(64 + (i % 4) as i32));
        let same: Heights = std::array::from_fn(|i| Some(66 + (i % 4) as i32));
        let other: Heights = std::array::from_fn(|_| Some(120));
        assert_eq!(similarity(&real, &same), Some(1.0));
        assert_eq!(similarity(&real, &other), Some(0.0));
        assert_eq!(similarity(&[None; 256], &[None; 256]), None);
    }

    /// Verdict follows median score.
    #[test]
    fn median_decides() {
        assert!(verdict(&mut [0.9, 0.1, 0.8]).1);
        assert!(!verdict(&mut [0.2, 0.1, 0.9]).1);
    }
}
