# Horizon Mod Template (Veinminer)

Official starter template for [Horizon](https://github.com/horizon-rust/horizon) code mods built on `horizon-mod-api` and the exact-build Mod SDK.

The template implements a complete, working **server-side Veinminer**: hold **Sneak** in Survival mode and break a connected ore vein (including stone + deepslate pairs) or tree log to mine up to 64 connected blocks at once. Every block is broken through `PlayerRef::destroy_block`, preserving vanilla tool checks, enchantments, tool wear, loot tables, statistics, exhaustion, and spawn protection. Drops and experience are collected via `Hooks::block_drops`, merged, and spawned at the mined block.

The same Rust source builds as either:
- a **native `.hmod`** (`kind = "native-mod"`, compiled by the Horizon launcher against the signed SDK), or
- a **sandboxed WebAssembly `.hmod`** (`kind = "mod"`, built to `mod.wasm` with `cargo horizon-mod build --wasm`, no native-code trust prompt needed).

> **Pinned Horizon build:** Nightly 2026-11-22 · `f1bb66f` (`f1bb66f353cf67f2aa4665293fc75c8d91186fc7`, SDK release `sdk-f1bb66f353cf67f2aa4665293fc75c8d91186fc7`).

## Prerequisites

1. [Rust](https://rustup.rs) (`cargo`) and the [Horizon launcher](https://playhorizon.dev).
2. Install `cargo-horizon-mod` from the pinned Horizon commit:

```sh
cargo install --locked --git https://github.com/horizon-rust/horizon --rev f1bb66f353cf67f2aa4665293fc75c8d91186fc7 cargo-horizon-mod
```

## Quick start (native `.hmod`)

Click **Use this template → Create a new repository**, clone it, and fetch the pinned SDK into `.horizon/sdk/`:

```sh
# Fetches and verifies the signed SDK into .horizon/sdk/ (not committed to git)
cargo horizon-mod check

# Run unit tests and lints
cargo test
cargo clippy --all-targets -- -D warnings

# Pack target/horizon-mod/veinminer.hmod and verify the package
cargo horizon-mod build
cargo horizon-mod check target/horizon-mod/veinminer.hmod
```

To build and play it in a Horizon launcher instance:

```sh
horizon-launcher native-build . --instance "My Instance" --trust-native-code
```

## Building the same code as WebAssembly (`.hmod`)

Because this template uses only `horizon-mod-api` (no mixins or game internals), the exact same `src/` compiles to a sandboxed `mod.wasm`:

1. Remove the `[native]` table from `horizon-mod.toml` (a mod carries `[native]` or `[wasm]`, never both).
2. Add the SDK pin to `Cargo.toml`:
   ```toml
   [package.metadata.horizon]
   sdk = "f1bb66f353cf67f2aa4665293fc75c8d91186fc7"
   ```
3. Build and check:
   ```sh
   cargo horizon-mod build --wasm
   cargo horizon-mod check target/horizon-mod/veinminer.hmod
   ```

Copy `target/horizon-mod/veinminer.hmod` into your instance's `horizon-mods/` folder and enable it in `horizon-mods.txt` (`mod.veinminer:true`).

## Project layout

```text
horizon-mod.toml      # Mod identity (id, version, side) and pinned Horizon build
Cargo.toml            # Depends on horizon-mod-api from .horizon/sdk/
src/
  lib.rs              # #[mod_api::horizon_mod] Hooks impl (break_block + block_drops)
  vein.rs             # Pure vein-group definitions, 26-connected BFS, and unit tests
assets/               # Optional (create if adding client resources: textures, models, lang)
data/                 # Optional (create if adding server data: recipes, loot tables, tags)
.horizon/sdk/         # Fetched & verified signed SDK (gitignored, never packed)
```

## Customizing for your own mod

1. Update `id`, `name`, `description`, `authors`, and `side` in `horizon-mod.toml` (`id` must match `[a-z0-9_]{1,64}`).
2. Update `name` and `description` in `Cargo.toml`.
3. Replace or extend `src/lib.rs` and `src/vein.rs` with your hooks — see the [Horizon Modding Book](https://github.com/horizon-rust/horizon/tree/main/docs/modding/src) for available hooks, `Host` methods, Parts, attachments, and optional `[[inject]]` / `[[native.mixin]]` escape hatches.
4. Do **not** add a `rust-toolchain.toml`: the pinned Horizon SDK controls the toolchain that builds native and WebAssembly mods.

## Updating to a newer Horizon Nightly

```sh
cargo horizon-mod update
```

Or pin an explicit build commit:

```sh
cargo horizon-mod update --horizon <40-char-commit-sha>
```

## Publishing to `horizon-mods`

1. Run `cargo horizon-mod check target/horizon-mod/<id>.hmod`.
2. Attach `<id>-<version>.hmod` to a GitHub release of your repository.
3. Open a pull request on [horizon-rust/horizon-mods](https://github.com/horizon-rust/horizon-mods) adding `mods/<id>.toml` — see its [Submit guide](https://github.com/horizon-rust/horizon-mods#submit).

## License

[MIT](LICENSE)
