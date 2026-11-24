//! Pure vein-grouping and breadth-first search logic.
//!
//! Kept free of host calls so it is easy to read and unit test directly with `cargo test`.

use std::collections::{HashSet, VecDeque};

/// Maximum number of blocks broken in a single vein-mine action (including the origin block).
pub const MAX_BLOCKS: usize = 64;

/// Maximum Chebyshev distance from the origin block in any axis.
pub const MAX_RADIUS: i32 = 16;

/// Logical block families that vein-mine together.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum VeinGroup {
    CoalOre,
    IronOre,
    CopperOre,
    GoldOre,
    RedstoneOre,
    EmeraldOre,
    LapisOre,
    DiamondOre,
    NetherQuartzOre,
    AncientDebris,
    OakLog,
    SpruceLog,
    BirchLog,
    JungleLog,
    AcaciaLog,
    DarkOakLog,
    MangroveLog,
    CherryLog,
    PaleOakLog,
    CrimsonStem,
    WarpedStem,
}

/// Maps a block's registry ID (for example `"minecraft:deepslate_iron_ore"`) to its vein group.
///
/// Stone and deepslate variants of the same ore share one group so a vein crossing the
/// stone/deepslate boundary mines cleanly as a single vein.
pub fn vein_group_for_block(block_id: &str) -> Option<VeinGroup> {
    match block_id {
        "minecraft:coal_ore" | "minecraft:deepslate_coal_ore" => Some(VeinGroup::CoalOre),
        "minecraft:iron_ore" | "minecraft:deepslate_iron_ore" => Some(VeinGroup::IronOre),
        "minecraft:copper_ore" | "minecraft:deepslate_copper_ore" => Some(VeinGroup::CopperOre),
        "minecraft:gold_ore" | "minecraft:deepslate_gold_ore" | "minecraft:nether_gold_ore" => {
            Some(VeinGroup::GoldOre)
        }
        "minecraft:redstone_ore" | "minecraft:deepslate_redstone_ore" => {
            Some(VeinGroup::RedstoneOre)
        }
        "minecraft:emerald_ore" | "minecraft:deepslate_emerald_ore" => Some(VeinGroup::EmeraldOre),
        "minecraft:lapis_ore" | "minecraft:deepslate_lapis_ore" => Some(VeinGroup::LapisOre),
        "minecraft:diamond_ore" | "minecraft:deepslate_diamond_ore" => Some(VeinGroup::DiamondOre),
        "minecraft:nether_quartz_ore" => Some(VeinGroup::NetherQuartzOre),
        "minecraft:ancient_debris" => Some(VeinGroup::AncientDebris),
        "minecraft:oak_log" => Some(VeinGroup::OakLog),
        "minecraft:spruce_log" => Some(VeinGroup::SpruceLog),
        "minecraft:birch_log" => Some(VeinGroup::BirchLog),
        "minecraft:jungle_log" => Some(VeinGroup::JungleLog),
        "minecraft:acacia_log" => Some(VeinGroup::AcaciaLog),
        "minecraft:dark_oak_log" => Some(VeinGroup::DarkOakLog),
        "minecraft:mangrove_log" => Some(VeinGroup::MangroveLog),
        "minecraft:cherry_log" => Some(VeinGroup::CherryLog),
        "minecraft:pale_oak_log" => Some(VeinGroup::PaleOakLog),
        "minecraft:crimson_stem" => Some(VeinGroup::CrimsonStem),
        "minecraft:warped_stem" => Some(VeinGroup::WarpedStem),
        _ => None,
    }
}

/// Collects up to `max_blocks` 26-connected positions (including diagonals) within `max_radius`
/// of `origin` for which `matches(pos)` returns `true`.
///
/// The returned list always starts with `origin` and contains each position at most once,
/// ordered by BFS distance from `origin`.
pub fn collect_vein<F>(
    origin: [i32; 3],
    max_blocks: usize,
    max_radius: i32,
    mut matches: F,
) -> Vec<[i32; 3]>
where
    F: FnMut([i32; 3]) -> bool,
{
    if max_blocks == 0 {
        return Vec::new();
    }

    let mut result = Vec::with_capacity(max_blocks.min(64));
    let mut visited = HashSet::with_capacity(max_blocks.min(64));
    let mut queue = VecDeque::new();

    visited.insert(origin);
    queue.push_back(origin);

    while let Some(pos) = queue.pop_front() {
        result.push(pos);
        if result.len() >= max_blocks {
            break;
        }

        for dx in -1..=1 {
            for dy in -1..=1 {
                for dz in -1..=1 {
                    if dx == 0 && dy == 0 && dz == 0 {
                        continue;
                    }
                    let next = [pos[0] + dx, pos[1] + dy, pos[2] + dz];
                    if !within_radius(origin, next, max_radius) || visited.contains(&next) {
                        continue;
                    }
                    visited.insert(next);
                    if matches(next) {
                        queue.push_back(next);
                    }
                }
            }
        }
    }

    result
}

fn within_radius(origin: [i32; 3], pos: [i32; 3], max_radius: i32) -> bool {
    (pos[0] - origin[0]).abs() <= max_radius
        && (pos[1] - origin[1]).abs() <= max_radius
        && (pos[2] - origin[2]).abs() <= max_radius
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn groups_stone_and_deepslate_ores_together() {
        assert_eq!(
            vein_group_for_block("minecraft:diamond_ore"),
            vein_group_for_block("minecraft:deepslate_diamond_ore")
        );
        assert_ne!(
            vein_group_for_block("minecraft:iron_ore"),
            vein_group_for_block("minecraft:gold_ore")
        );
        assert_eq!(vein_group_for_block("minecraft:stone"), None);
    }

    #[test]
    fn bfs_follows_diagonal_neighbors_and_respects_limit() {
        let blocks: HashSet<[i32; 3]> = (0..10).map(|i| [i, i, 0]).collect();
        let vein = collect_vein([0, 0, 0], 5, MAX_RADIUS, |pos| blocks.contains(&pos));
        assert_eq!(
            vein,
            vec![[0, 0, 0], [1, 1, 0], [2, 2, 0], [3, 3, 0], [4, 4, 0]]
        );
    }

    #[test]
    fn bfs_respects_radius_bound() {
        let vein = collect_vein([0, 0, 0], MAX_BLOCKS, 2, |pos| pos[1] == 0 && pos[2] == 0);
        assert_eq!(vein.len(), 5);
        assert!(vein.iter().all(|pos| pos[0].abs() <= 2));
    }
}
