//! One level's seeded generation, shared with the far-terrain threads.

use std::collections::HashMap;
use std::sync::Mutex;
use std::sync::atomic::{AtomicI32, AtomicU64, Ordering};

use crate::verify::{self, Heights};

pub struct Run {
    pub server: String,
    pub level: String,
    pub seed: i64,
    /// The seed's hash is the one the server sent.
    pub hash_matches: bool,
    center: [AtomicI32; 2],
    generated: AtomicU64,
    check: Mutex<Check>,
}

#[derive(Default)]
struct Check {
    running: bool,
    /// Server chunks waiting for their generated twin.
    real: HashMap<(i32, i32), Heights>,
    /// Chunks to ask the generator for.
    wanted: Vec<(i32, i32)>,
    scores: Vec<f32>,
    result: Option<(f32, bool)>,
}

impl Run {
    pub fn new(server: &str, level: &str, seed: i64, hash_matches: bool) -> Self {
        Self {
            server: server.to_owned(),
            level: level.to_owned(),
            seed,
            hash_matches,
            center: [AtomicI32::new(0), AtomicI32::new(0)],
            generated: AtomicU64::new(0),
            // Only a seed the server's hash doesn't vouch for is compared.
            check: Mutex::new(Check {
                running: !hash_matches,
                ..Check::default()
            }),
        }
    }

    pub fn set_center(&self, [x, z]: [i32; 2]) {
        self.center[0].store(x, Ordering::Relaxed);
        self.center[1].store(z, Ordering::Relaxed);
    }

    pub fn center(&self) -> [i32; 2] {
        self.center.each_ref().map(|c| c.load(Ordering::Relaxed))
    }

    /// Whether chunk `(x, z)` is within `/lodseed radius` of the player.
    pub fn in_radius(&self, x: i32, z: i32) -> bool {
        let radius = crate::radius();
        let [cx, cz] = self.center();
        radius <= 0 || ((x - cx).abs() <= radius && (z - cz).abs() <= radius)
    }

    pub fn generated_one(&self) {
        self.generated.fetch_add(1, Ordering::Relaxed);
    }

    pub fn generated(&self) -> u64 {
        self.generated.load(Ordering::Relaxed)
    }

    /// The soft-compare's result so far: `None` while running or skipped.
    pub fn result(&self) -> Option<(f32, bool)> {
        self.check.lock().unwrap().result
    }

    pub fn checking(&self) -> bool {
        self.check.lock().unwrap().running
    }

    /// A chunk the server sent: kept to compare, until enough are.
    pub fn offer_real(&self, key: (i32, i32), heights: Heights) {
        let mut check = self.check.lock().unwrap();
        if check.running
            && check.real.len() + check.scores.len() < verify::CHUNKS * 2
            && !check.real.contains_key(&key)
        {
            check.real.insert(key, heights);
            check.wanted.push(key);
        }
    }

    pub fn take_wanted(&self) -> Vec<(i32, i32)> {
        std::mem::take(&mut self.check.lock().unwrap().wanted)
    }

    pub fn is_compared(&self, key: (i32, i32)) -> bool {
        self.check.lock().unwrap().real.contains_key(&key)
    }

    /// The generated twin of a kept server chunk.
    pub fn compare(&self, key: (i32, i32), generated: &Heights) {
        let mut check = self.check.lock().unwrap();
        let Some(real) = check.real.remove(&key) else {
            return;
        };
        if let Some(score) = verify::similarity(&real, generated) {
            check.scores.push(score);
        }
        if !check.running || check.scores.len() < verify::CHUNKS {
            return;
        }
        let (score, pass) = verify::verdict(&mut check.scores);
        check.running = false;
        check.real.clear();
        check.result = Some((score, pass));
        drop(check);
        let percent = (score * 100.0).round();
        crate::say(&if pass {
            format!(
                "Terrain matches seed {} ({percent}% alike). The server just hides its seed hash.",
                self.seed
            )
        } else {
            format!(
                "Terrain doesn't match seed {} ({percent}% alike). Check the seed, then run /lodseed set <seed> again.",
                self.seed
            )
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compares_until_enough_chunks() {
        let run = Run::new("s", "minecraft:overworld", 1, false);
        let flat: Heights = [Some(64); 256];
        for i in 0..verify::CHUNKS as i32 {
            run.offer_real((i, 0), flat);
        }
        assert_eq!(run.take_wanted().len(), verify::CHUNKS);
        for i in 0..verify::CHUNKS as i32 {
            assert!(run.is_compared((i, 0)));
            run.compare((i, 0), &flat);
        }
        assert_eq!(run.result(), Some((1.0, true)));
        assert!(!run.checking());
    }

    #[test]
    fn a_vouched_seed_is_not_compared() {
        let run = Run::new("s", "minecraft:overworld", 1, true);
        run.offer_real((0, 0), [Some(64); 256]);
        assert!(run.take_wanted().is_empty());
    }

    #[test]
    fn radius_follows_the_player() {
        crate::set_radius(4);
        let run = Run::new("s", "minecraft:overworld", 1, true);
        run.set_center([10, -10]);
        assert!(run.in_radius(14, -6));
        assert!(!run.in_radius(15, -10));
        crate::set_radius(0);
        assert!(run.in_radius(1000, 1000));
    }
}
