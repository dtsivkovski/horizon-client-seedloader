# Client Seed Loader

A client-side [Horizon](https://playhorizon.dev) mod. On a multiplayer server, give it the world seed and Horizon's far terrain (LODs) will generate chunks you haven't been to. It's only generated locally from a provided seed, so you can take advantage of Horizon's fast LOD rendering.

- Chunks the server has already sent take priority over locally generated ones.
- Generated chunks are cached on disk, loading instantly the next time.
- Nothing is sent back to the server.

## Use

In chat, on a server:

| Command | Does |
| --- | --- |
| `/lodseed set <seed> [dimension]` | Save the seed for this server (may specify a specific dimension, otherwise it's all of them) |
| `/lodseed clear [dimension]` | Forget the seed |
| `/lodseed radius <chunks>` | Limit generation around you (`0` = far-terrain distance) |
| `/lodseed status` | Seed, hash check, terrain check, chunks generated |

Far terrain must be on (Video Settings). Seeds can be numbers or text, just like for world gen.

**Seed check:** servers send a hash of their seed. If yours doesn't match, the mod warns you, then compares a few generated chunks with real ones and tells you whether the terrain matches (this is because some servers may fake the hash).

## Files

| Path (in the game folder) | What |
| --- | --- |
| `config/client_seed_loader.toml` | Seeds per server and dimension |
| `horizon-mod-data/client_seed_loader/<server>/<hashed seed>/<seed>/<dimension>/` | Generated chunk cache (safe to delete) |
| `server-terrain/` | Horizon's own cache of real server chunks (untouched) |

## Develop

Needs [Rust](https://rustup.rs) and the Horizon launcher.

```sh
# Once, and after `cargo horizon-mod update`: install the build tool
cargo install --locked --git https://github.com/horizon-rust/horizon \
  --rev "$(sed -n 's/^horizon = "\(.*\)"/\1/p' horizon-mod.toml)" cargo-horizon-mod

cargo horizon-mod check        # fetch the SDK into .horizon/sdk/
cargo test                     # unit tests (seed, config, terrain compare)
cargo clippy --all-targets -- -D warnings
cargo horizon-mod build        # pack target/horizon-mod/client_seed_loader.hmod

# Compile against the game and install into an instance, then play it
horizon-launcher native-build . --instance "<instance>" --trust-native-code
```

`cargo build` only checks `src/lib.rs` and its modules. The game-side code (`src/game/`, `src/mixins/`) compiles only inside `native-build`.

### Layout

```text
horizon-mod.toml     # id, pinned Horizon build, the mixin and support module
src/lib.rs           # /lodseed commands, chat lines, state shared with the game side
src/config.rs        # config/client_seed_loader.toml
src/seed.rs          # text seeds -> numbers
src/verify.rs        # soft-compare of generated vs real chunks
src/run.rs           # one level's generation state
src/game/seeded.rs   # the far-terrain source (compiled inside the game)
src/mixins/          # hooks it into Game::prepare_lod
```

### How it works

Horizon picks a far-terrain source in `Game::prepare_lod`. On a plain server that's `ServerTerrain`, which only knows chunks the server sent. A `HEAD` mixin there swaps in a source that asks `ServerTerrain` first, then the disk cache, then Horizon's own vanilla generator (`VanillaTerrain`, the one single-player uses) on its low-priority far-terrain threads.

### Updating Horizon

```sh
cargo horizon-mod update
```

Then reinstall the build tool (above) and rebuild. If `prepare_lod` changed upstream, compare it with `src/game/seeded.rs`, which mirrors its checks.

## License

[MIT](LICENSE)
