// Seeded far terrain on servers, built into game's `world_update` module.
// - Server chunks always win (Horizon's `server-terrain/` store).
// - Missing chunks come from seed, made by Horizon's generator on low-priority far-terrain threads.
// - Generated chunks are cached in `horizon-mod-data/client_seed_loader/<server>/<hashed seed>/<seed>/<dimension>/`.

use client_seed_loader::{Heights, Run};
use horizon_client::lod::Column;
use horizon_client::server_terrain::{self as terrain, ServerTerrain};
use horizon_client::streaming::{LodDetail, LodSampler, LodSource};
use horizon_client::vanillagen::{TerrainResources, VanillaTerrain};
use std::collections::HashSet;
use std::ops::ControlFlow;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

type Columns = Arc<[Column; 256]>;

/// Dimensions vanilla generator knows.
const LEVELS: [&str; 3] = [
    "minecraft:overworld",
    "minecraft:the_nether",
    "minecraft:the_end",
];

/// Runs first in `prepare_lod`. `Break` if seeded far terrain got set up, else `Continue`.
pub fn prepare_lod(game: &mut super::Game<'_>) -> ControlFlow<()> {
    let (address, level, hashed) = match (&game.remote, &game.integrated) {
        (Some(remote), None) => (
            remote.session.address.clone(),
            remote.level.clone(),
            remote.hashed_seed,
        ),
        _ => {
            client_seed_loader::left();
            return ControlFlow::Continue(());
        }
    };
    let server = terrain::server_folder(&address);
    client_seed_loader::here(&server, &level, hashed);
    if client_seed_loader::take_dirty() {
        game.clear_lod();
    }
    // Leave every non-`ServerTerrain` case to `prepare_lod`.
    let enabled = game.far_terrain_choice().0 > 0
        && game.renderer.is_some()
        && game.player.is_some()
        && game.title.is_none();
    if !enabled
        || game.options.split_guest
        || address.starts_with(horizon_client::net::FRIEND)
        || game.server_far_terrain().is_some()
        || !LEVELS.contains(&level.as_str())
    {
        return ControlFlow::Continue(());
    }
    if game.lod_level.as_ref().is_some_and(|l| *l != level) {
        game.clear_lod();
    }
    if game.lod_source_attempted {
        return ControlFlow::Continue(());
    }
    let Some(seed) = client_seed_loader::seed_for(&server, &level) else {
        return ControlFlow::Continue(());
    };
    let Some(source) = seeded_source(game, &address, &level, hashed, seed) else {
        return ControlFlow::Continue(());
    };
    // Install source same way `prepare_lod` does.
    game.lod_source_attempted = true;
    game.lod_level = Some(level);
    let lod = horizon_client::lod::Lod::new(source, game.worker_context());
    lod.set_looks(game.streaming.looks, game.streaming.nether_shading);
    lod.set_shader_pack(game.streaming.shader_pack);
    game.lod = Some(lod);
    game.lod_observe.extend(game.world.chunks.keys().copied());
    ControlFlow::Break(())
}

/// Builds seeded source for this level. `None` if generator can't load.
fn seeded_source(
    game: &super::Game<'_>,
    address: &str,
    level: &str,
    hashed: i64,
    seed: i64,
) -> Option<Arc<dyn LodSource>> {
    let home = std::env::current_dir().ok()?;
    let remote = game.remote.as_ref()?;

    // Horizon's kept chunks, opened same way `prepare_lod` does.
    let real_dir = terrain::level_dir(&home.join("server-terrain"), address, hashed, level);
    let bobby = terrain::bobby_level_dir(
        &terrain::minecraft_dir().join(".bobby"),
        address,
        hashed,
        level,
    )
    .map(|dir| {
        let registry = game.pack.assets.registry;
        let biomes = game.res.biomes().clone();
        let layout = horizon_client::chunk::ColumnLayout::of_level(
            remote.dimension.min_y,
            remote.dimension.height,
        );
        let sky_light = remote.dimension.has_skylight;
        let decode: terrain::DecodeChunk = Box::new(move |root: &horizon_core::nbt::NamedTag| {
            horizon_client::save::decode_lit(root, registry, &biomes, layout, sky_light)
        });
        terrain::Bobby::new(dir, decode, game.res.facts())
    });
    let real = ServerTerrain::with_bobby(real_dir, bobby);

    // Our cache. Args shift by one to get <server>/<hashed seed>/<seed>/<dimension>.
    let ours = home
        .join("horizon-mod-data/client_seed_loader")
        .join(terrain::server_folder(address));
    let cache = ServerTerrain::new(terrain::level_dir(&ours, &hashed.to_string(), seed, level));

    let worldgen = game.options.resources.join("worldgen");
    let biomes = match &game.world_biomes {
        Some(biomes) => biomes,
        None => game.res.biomes(),
    };
    let resources = TerrainResources {
        data: Some(&game.pack.data_changes),
        dir: &worldgen,
        registry: game.pack.assets.registry,
        biomes,
        item_data: &game.res.core.item_data,
    };
    let generator = match VanillaTerrain::for_dimension(resources, seed, level) {
        Ok(generator) => Arc::new(generator),
        Err(e) => {
            log::warn!("Client Seed Loader: no generator for {level}: {e}");
            client_seed_loader::say(&format!("Couldn't generate {level}: {e}"));
            return None;
        }
    };

    let hash_matches = horizon_core::biome::obfuscate_seed(seed) == hashed;
    let run =
        client_seed_loader::start(&terrain::server_folder(address), level, seed, hash_matches);
    Some(Arc::new(Seeded {
        real,
        cache,
        generator,
        run,
        warned: AtomicBool::new(false),
    }))
}

struct Seeded {
    real: Arc<ServerTerrain>,
    cache: Arc<ServerTerrain>,
    generator: Arc<VanillaTerrain>,
    run: Arc<Run>,
    warned: AtomicBool,
}

impl LodSource for Seeded {
    /// Sampler over real chunks, cache, and generator.
    fn sampler(self: Arc<Self>) -> Box<dyn LodSampler> {
        let real = self.real.clone().sampler();
        let generated = LodSource::sampler(self.generator.clone());
        Box::new(Sampler {
            source: self,
            real,
            generated,
            details: Vec::new(),
            wanted: HashSet::new(),
        })
    }
    /// Redraws tiles once real server chunks arrive.
    fn keeps_observed(&self) -> bool {
        true
    }
}

struct Sampler {
    source: Arc<Seeded>,
    real: Box<dyn LodSampler>,
    generated: Box<dyn LodSampler>,
    details: Vec<LodDetail>,
    /// Chunks far terrain asked for, still being generated.
    wanted: HashSet<(i32, i32)>,
}

impl Sampler {
    /// Cell known without generating (server's, cached, or out of range).
    fn known(&mut self, x: i32, z: i32, width: i32) -> Option<Column> {
        let s = &self.source;
        if width <= 16 {
            let (cx, cz) = (x >> 4, z >> 4);
            if let Some(chunk) = s.real.chunk(cx, cz) {
                return Some(reduced(&chunk, x, z, width));
            }
            if !s.run.in_radius(cx, cz) {
                return Some(Column::default());
            }
            return s
                .cache
                .chunk(cx, cz)
                .map(|chunk| reduced(&chunk, x, z, width));
        }
        let real = self.real.reduced_column(x, z, width);
        let middle = ((x + width / 2) >> 4, (z + width / 2) >> 4);
        (real != Column::default() || !s.run.in_radius(middle.0, middle.1)).then_some(real)
    }

    /// Requests generated twins of server chunks held for soft-compare.
    fn request_compared(&mut self) {
        let wanted = self.source.run.take_wanted();
        if !wanted.is_empty() {
            self.generated
                .request_detail(&wanted, self.source.run.center());
        }
    }
}

impl LodSampler for Sampler {
    /// Copy for another far-terrain thread.
    fn fork(&self) -> Option<Box<dyn LodSampler>> {
        Some(Box::new(Self {
            source: self.source.clone(),
            real: self.real.fork()?,
            generated: self.generated.fork()?,
            details: Vec::new(),
            wanted: HashSet::new(),
        }))
    }

    /// Serves known chunks now. Sends missing ones to generator.
    fn request_detail(&mut self, chunks: &[(i32, i32)], center: [i32; 2]) {
        let s = self.source.clone();
        s.run.set_center(center);
        let mut missing = Vec::new();
        for &(x, z) in chunks {
            let known = s.real.chunk(x, z).or_else(|| {
                if s.run.in_radius(x, z) {
                    s.cache.chunk(x, z)
                } else {
                    Some(empty())
                }
            });
            match known {
                Some(columns) => self.details.push(((x, z), columns)),
                None => {
                    self.wanted.insert((x, z));
                    missing.push((x, z));
                }
            }
        }
        if !missing.is_empty() {
            self.generated.request_detail(&missing, center);
        }
        self.request_compared();
    }

    /// Finished chunks. Caches generated ones and feeds soft-compare.
    fn drain_details(&mut self) -> Vec<LodDetail> {
        self.request_compared();
        let s = self.source.clone();
        for (key, columns) in self.generated.drain_details() {
            if s.run.is_compared(key) {
                s.run.compare(key, &heights(&columns));
            }
            if !self.wanted.remove(&key) {
                continue;
            }
            // Server sent it meanwhile.
            if let Some(real) = s.real.chunk(key.0, key.1) {
                self.details.push((key, real));
                continue;
            }
            match s.cache.store(key, &columns) {
                Ok(()) => s.run.generated_one(),
                Err(e) if !s.warned.swap(true, Ordering::Relaxed) => {
                    log::warn!("Client Seed Loader: can't cache generated chunks: {e}");
                }
                Err(_) => {}
            }
            self.details.push((key, columns));
        }
        std::mem::take(&mut self.details)
    }

    /// Server sent a chunk. Stores it and offers it to soft-compare.
    fn observed(&mut self, key: (i32, i32), columns: &Columns) {
        self.real.observed(key, columns);
        self.source.run.offer_real(key, heights(columns));
    }

    /// Passes approximate mode on to both samplers.
    fn set_approximate(&mut self, approximate: bool) {
        self.real.set_approximate(approximate);
        self.generated.set_approximate(approximate);
    }

    /// Surface at block `(x, z)`.
    fn sample(&mut self, x: i32, z: i32) -> Option<horizon_client::lod::Surface> {
        self.column(x, z).surface
    }

    /// Column at block `(x, z)`.
    fn column(&mut self, x: i32, z: i32) -> Column {
        self.reduced_column(x, z, 1)
    }

    /// Cell of `width` blocks. Known data first, else generated.
    fn reduced_column(&mut self, x: i32, z: i32, width: i32) -> Column {
        match self.known(x, z, width) {
            Some(column) => column,
            None => self.generated.reduced_column(x, z, width),
        }
    }

    /// Grid of cells. Generates only unknown ones, in one batch.
    fn grid(&mut self, x: i32, z: i32, stride: i32, side: usize) -> Vec<Column> {
        let cells: Vec<Option<Column>> = (0..side * side)
            .map(|i| {
                let (dx, dz) = ((i % side) as i32, (i / side) as i32);
                self.known(x + dx * stride, z + dz * stride, stride)
            })
            .collect();
        if cells.iter().all(Option::is_some) {
            return cells.into_iter().flatten().collect();
        }
        let generated = self.generated.grid(x, z, stride, side);
        // Empty means sampling failed. Retried later.
        if generated.len() != cells.len() {
            return generated;
        }
        cells
            .into_iter()
            .zip(generated)
            .map(|(known, generated)| known.unwrap_or(generated))
            .collect()
    }
}

/// Cell of `width` blocks from one chunk, reduced like `ServerTerrain` does.
fn reduced(chunk: &[Column; 256], x: i32, z: i32, width: i32) -> Column {
    let width = width.max(1) as usize;
    let lx = (x & 15) as usize & !(width - 1);
    let lz = (z & 15) as usize & !(width - 1);
    Column::reduced(chunk, lx, lz, width)
}

/// Chunk with no terrain, for out-of-radius requests.
fn empty() -> Columns {
    Arc::new(std::array::from_fn(|_| Column::default()))
}

/// Top heights for soft-compare (solid face, or fluid above it).
fn heights(columns: &[Column; 256]) -> Heights {
    std::array::from_fn(|i| {
        columns[i]
            .surface
            .map(|s| s.water.map_or(s.height, |(h, _)| h.max(s.height)))
    })
}
