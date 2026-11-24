//! Horizon mod template: server-side Veinminer on `horizon-mod-api`.
//!
//! Hold sneak in Survival mode and break a matching ore or log to mine its connected vein.
//! Every extra break goes through [`PlayerRef::destroy_block`], preserving vanilla tool checks,
//! durability wear, enchantments, loot tables, statistics, exhaustion, and spawn protection.
//! Drops and experience from the vein are collected via [`Hooks::block_drops`], merged, and
//! spawned at the block the player mined.
//!
//! The same crate builds as a native `.hmod` (default) or a sandboxed WebAssembly `.hmod`
//! without changing any Rust source.

mod vein;

use std::collections::HashSet;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

use mod_api::{
    self, BlockDropsEvent, BreakBlockEvent, EntityRef, Flow, GameMode, Hooks, ItemStack, LevelHost,
    ServerHost,
};

use crate::vein::{MAX_BLOCKS, MAX_RADIUS, collect_vein, vein_group_for_block};

#[derive(Default)]
struct Veinminer {
    busy: AtomicBool,
    active: Mutex<Option<ActiveVein>>,
}

struct ActiveVein {
    breaker: EntityRef,
    positions: HashSet<[i32; 3]>,
    drops: Vec<ItemStack>,
    experience: u32,
}

struct BusyGuard<'a>(&'a AtomicBool);

impl Drop for BusyGuard<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Relaxed);
    }
}

#[mod_api::horizon_mod]
impl Hooks for Veinminer {
    fn break_block(&self, host: &mut dyn ServerHost, event: &mut BreakBlockEvent<'_>) -> Flow {
        // Our own `player.destroy_block` calls re-enter `break_block`; let them run normally.
        if self.busy.load(Ordering::Relaxed) {
            return Flow::Continue;
        }

        let Some(player) = host.players().acting_player() else {
            return Flow::Continue;
        };
        if player.game_mode(host) != Some(GameMode::Survival) {
            return Flow::Continue;
        }
        if player.is_secondary_use_active(host) != Some(true) {
            return Flow::Continue;
        }

        let Some(level) = player.level(host) else {
            return Flow::Continue;
        };
        let origin = event.pos;
        let Some(origin_block) = level.block(host, origin) else {
            return Flow::Continue;
        };
        let Some(group) = vein_group_for_block(&origin_block.id(host)) else {
            return Flow::Continue;
        };

        let vein = collect_vein(origin, MAX_BLOCKS, MAX_RADIUS, |pos| {
            level
                .block(host, pos)
                .and_then(|block| vein_group_for_block(&block.id(host)))
                == Some(group)
        });

        // Single block: let vanilla finish the break directly.
        if vein.len() <= 1 {
            return Flow::Continue;
        }

        if self.busy.swap(true, Ordering::Relaxed) {
            return Flow::Continue;
        }
        let _guard = BusyGuard(&self.busy);

        let positions: HashSet<[i32; 3]> = vein.iter().copied().collect();
        *self.active.lock().unwrap() = Some(ActiveVein {
            breaker: EntityRef::from(player),
            positions,
            drops: Vec::new(),
            experience: 0,
        });

        for pos in vein {
            // Break each block as the player (origin first). Stop if a break is refused
            // (for example, spawn protection or a tool constraint) only on the origin;
            // continue across the remaining blocks in the vein otherwise.
            let broken = player.destroy_block(host, pos, false);
            if pos == origin && !broken {
                let _ = self.active.lock().unwrap().take();
                return Flow::Continue;
            }
        }

        if let Some(active) = self.active.lock().unwrap().take() {
            for stack in merge_stacks(host, active.drops) {
                let _ = level.pop_resource(host, origin, stack);
            }
            if active.experience > 0 {
                let [x, y, z] = origin.map(|v| f64::from(v) + 0.5);
                level.award_experience(host, x, y, z, active.experience);
            }
        }

        // The origin block was already broken by the loop above.
        Flow::Cancel
    }

    fn block_drops(&self, _: &dyn LevelHost, event: &mut BlockDropsEvent<'_>) -> Flow {
        if !self.busy.load(Ordering::Relaxed) {
            return Flow::Continue;
        }

        let pos = [event.pos.x, event.pos.y, event.pos.z];
        let mut guard = self.active.lock().unwrap();
        if let Some(active) = guard.as_mut()
            && event.breaker_entity == Some(active.breaker)
            && active.positions.contains(&pos)
        {
            active
                .drops
                .extend(event.drops.drain(..).map(|drop| drop.stack));
            active.experience = active
                .experience
                .saturating_add(std::mem::take(&mut event.experience));
        }

        Flow::Continue
    }
}

/// Folds stackable drops together up to each item's maximum stack size, preserving order.
fn merge_stacks(host: &dyn ServerHost, stacks: Vec<ItemStack>) -> Vec<ItemStack> {
    let mut merged: Vec<ItemStack> = Vec::new();
    for mut stack in stacks {
        let max_stack = host.max_stack_size(stack.item()).unwrap_or(64);
        let mut remaining = stack.count();
        for existing in &mut merged {
            if remaining == 0 {
                break;
            }
            if existing.count() < max_stack && existing.is_stackable_with(&stack) {
                let move_count = (max_stack - existing.count()).min(remaining);
                if existing.set_count(existing.count() + move_count).is_ok() {
                    remaining -= move_count;
                }
            }
        }
        if remaining > 0 && (remaining == stack.count() || stack.set_count(remaining).is_ok()) {
            merged.push(stack);
        }
    }
    merged
}
