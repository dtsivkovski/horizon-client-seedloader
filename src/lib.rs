//! Client Seed Loader: Use a seed you know to generate far terrain on servers
//!
//! - Keeps seeds (`/lodseed`, `config/client_seed_loader.toml`) and chat lines.
//! - Far-terrain source is `src/game/seeded.rs`, compiled inside game. It calls `pub fn`s here.

mod config;
mod run;
mod seed;
mod verify;

use std::sync::atomic::{AtomicI32, Ordering};
use std::sync::{Arc, Mutex};

use mod_api::{
    ClientDisconnectingEvent, ClientHost, ClientTickPostEvent, Command, CommandEvent, Flow, Hooks,
    RegisterClientCommandsEvent, Text,
};

use crate::config::Config;
pub use crate::run::Run;
pub use crate::verify::Heights;

const PREFIX: &str = "[Seed Loader] ";

/// Player's current server and level.
#[derive(Clone, PartialEq, Eq)]
struct Here {
    server: String,
    level: String,
    hashed_seed: i64,
}

struct State {
    config: Option<Config>,
    here: Option<Here>,
    /// Seed changed, so far terrain gets rebuilt.
    dirty: bool,
    run: Option<Arc<Run>>,
    /// Chat lines queued for next client tick.
    outbox: Vec<String>,
}

static STATE: Mutex<State> = Mutex::new(State {
    config: None,
    here: None,
    dirty: false,
    run: None,
    outbox: Vec::new(),
});
static RADIUS: AtomicI32 = AtomicI32::new(0);

/// Config, loaded on first use.
fn config(state: &mut State) -> &mut Config {
    state.config.get_or_insert_with(|| {
        let config = Config::load();
        RADIUS.store(config.radius, Ordering::Relaxed);
        config
    })
}

// ---- Called by game side (src/game/seeded.rs) ----

/// Player is on `server` (its far-terrain folder name) in `level`.
pub fn here(server: &str, level: &str, hashed_seed: i64) {
    let mut state = STATE.lock().unwrap();
    let same = state
        .here
        .as_ref()
        .is_some_and(|h| h.server == server && h.level == level && h.hashed_seed == hashed_seed);
    if !same {
        state.here = Some(Here {
            server: server.to_owned(),
            level: level.to_owned(),
            hashed_seed,
        });
    }
}

/// Not on a server.
pub fn left() {
    let mut state = STATE.lock().unwrap();
    state.here = None;
    state.run = None;
}

/// Whether a seed changed since last call. Resets flag.
pub fn take_dirty() -> bool {
    std::mem::take(&mut STATE.lock().unwrap().dirty)
}

/// Saved seed for `level` on `server`, if any.
pub fn seed_for(server: &str, level: &str) -> Option<i64> {
    config(&mut STATE.lock().unwrap()).seed(server, level)
}

/// Starts seeded generation for a level.
/// - Rebuilds (like toggling a shader pack) reuse same run, so no repeat compare or message.
pub fn start(server: &str, level: &str, seed: i64, hash_matches: bool) -> Arc<Run> {
    let mut state = STATE.lock().unwrap();
    if let Some(run) = &state.run
        && run.server == server
        && run.level == level
        && run.seed == seed
    {
        return run.clone();
    }
    let run = Arc::new(Run::new(server, level, seed, hash_matches));
    state.run = Some(run.clone());
    state.outbox.push(if hash_matches {
        format!("Seed {seed} matches this server. Generating far terrain for {level}.")
    } else {
        format!(
            "Seed {seed} doesn't match this server's seed hash (some servers hide it). \
             Comparing generated terrain with the server's…"
        )
    });
    run
}

/// Queues a chat line, shown next client tick.
pub fn say(line: &str) {
    STATE.lock().unwrap().outbox.push(line.to_owned());
}

/// `/lodseed radius` in chunks. 0 means no limit.
pub fn radius() -> i32 {
    RADIUS.load(Ordering::Relaxed)
}

/// Sets radius directly, for tests.
#[cfg(test)]
fn set_radius(radius: i32) {
    RADIUS.store(radius, Ordering::Relaxed);
}

// ---- /lodseed ----

const USAGES: [&str; 5] = [
    "lodseed",
    "lodseed status",
    "lodseed set <seed:word> [dimension:id]",
    "lodseed clear [dimension:id]",
    "lodseed radius <chunks:integer>",
];

/// Runs `/lodseed` with `args`. Returns chat lines.
fn lodseed(args: &[&str]) -> Vec<String> {
    let mut state = STATE.lock().unwrap();
    let Some(here) = state.here.clone() else {
        return vec!["Join a multiplayer server first: seeds are kept per server.".into()];
    };
    match args {
        [] | ["status"] => status(&mut state, &here),
        ["set", seed, rest @ ..] if rest.len() <= 1 => {
            let Some(seed) = seed::parse(seed) else {
                return vec!["That isn't a seed.".into()];
            };
            let server = config(&mut state)
                .servers
                .entry(here.server.clone())
                .or_default();
            let target = match rest.first() {
                Some(dimension) => {
                    server.dimensions.insert((*dimension).to_owned(), seed);
                    format!("{} on {dimension}", here.server)
                }
                None => {
                    server.seed = Some(seed);
                    here.server.clone()
                }
            };
            saved(&mut state, format!("Seed {seed} saved for {target}."))
        }
        ["clear", rest @ ..] if rest.len() <= 1 => {
            let config = config(&mut state);
            match rest.first() {
                Some(dimension) => {
                    if let Some(server) = config.servers.get_mut(&here.server) {
                        server.dimensions.remove(*dimension);
                    }
                }
                None => {
                    config.servers.remove(&here.server);
                }
            }
            saved(&mut state, format!("Seed cleared for {}.", here.server))
        }
        ["radius", chunks] => {
            let Some(chunks) = chunks.parse::<i32>().ok().filter(|c| *c >= 0) else {
                return vec!["The radius is a number of chunks, 0 for no limit.".into()];
            };
            config(&mut state).radius = chunks;
            RADIUS.store(chunks, Ordering::Relaxed);
            saved(
                &mut state,
                format!("Radius set to {}.", radius_text(chunks)),
            )
        }
        _ => vec![format!("Usage: /{}", USAGES[1..].join(", /"))],
    }
}

/// Saves config and rebuilds far terrain with it.
fn saved(state: &mut State, line: String) -> Vec<String> {
    state.dirty = true;
    match config(state).save() {
        Ok(()) => vec![line],
        Err(e) => vec![line, format!("Couldn't save {}: {e}", config::PATH)],
    }
}

/// Lines for `/lodseed status`.
fn status(state: &mut State, here: &Here) -> Vec<String> {
    let mut lines = vec![format!("{} in {}", here.server, here.level)];
    let Some(seed) = config(state).seed(&here.server, &here.level) else {
        lines.push("No seed set. Use /lodseed set <seed>.".into());
        return lines;
    };
    lines.push(format!("Seed: {seed}"));
    let run = state
        .run
        .as_ref()
        .filter(|r| r.server == here.server && r.level == here.level && r.seed == seed);
    match run {
        None => lines.push("Not generating (far terrain off, or not this dimension).".into()),
        Some(run) => {
            lines.push(if run.hash_matches {
                "Hash: matches the server.".into()
            } else {
                "Hash: doesn't match the server's.".into()
            });
            if let Some((score, pass)) = run.result() {
                let verdict = if pass { "matches" } else { "doesn't match" };
                lines.push(format!(
                    "Terrain {verdict} ({}% alike).",
                    (score * 100.0).round()
                ));
            } else if run.checking() {
                lines.push("Terrain: comparing…".into());
            }
            lines.push(format!("Generated chunks: {}", run.generated()));
        }
    }
    lines.push(format!("Radius: {}", radius_text(radius())));
    lines
}

/// Radius as words for chat.
fn radius_text(chunks: i32) -> String {
    if chunks == 0 {
        "the far-terrain distance".into()
    } else {
        format!("{chunks} chunks")
    }
}

/// Shows a prefixed chat line.
fn chat(host: &mut dyn ClientHost, line: &str) {
    let _ = host.chat_line(Text::literal(format!("{PREFIX}{line}")));
}

#[derive(Default)]
struct SeedLoader;

#[mod_api::horizon_mod]
impl Hooks for SeedLoader {
    /// Registers `/lodseed` for tab completion.
    fn register_client_commands(
        &self,
        _: &mut dyn ClientHost,
        event: &mut RegisterClientCommandsEvent<'_>,
    ) {
        event.commands.extend(USAGES.map(Command::new));
    }

    /// Handles `/lodseed` locally. Other commands pass through.
    fn command(&self, host: &mut dyn ClientHost, event: &mut CommandEvent<'_>) -> Flow {
        let mut words = event.command.split_whitespace();
        if words.next() != Some("lodseed") {
            return Flow::Continue;
        }
        let args: Vec<&str> = words.collect();
        for line in lodseed(&args) {
            chat(host, &line);
        }
        // Handled locally, never sent to server.
        Flow::Cancel
    }

    /// Shows queued chat lines.
    fn client_tick_post(&self, host: &mut dyn ClientHost, _: &mut ClientTickPostEvent) {
        let lines = std::mem::take(&mut STATE.lock().unwrap().outbox);
        for line in lines {
            chat(host, &line);
        }
    }

    /// Forgets current server.
    fn client_disconnecting(&self, _: &dyn ClientHost, _: &mut ClientDisconnectingEvent) {
        left();
    }
}
