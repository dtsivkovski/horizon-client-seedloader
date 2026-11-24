fn prepare_lod(game: &mut Game<'_>) -> ::core::ops::ControlFlow<()> {
    crate::game::world_update::csl_seeded::prepare_lod(game)
}
