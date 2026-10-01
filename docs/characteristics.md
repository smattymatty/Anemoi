# Anemoi: characteristics

Tier: **timeless**. Holds the ranked weights and the named conflict, nothing
that decays. Named by the operator at grill 2026-10-01; sealed by the commit
that lands this file. The engine's shape is ODDs' ADR 0004 (A simulates, B
presents); a game's own characteristics never rank the engine.

## The five, ranked (operator, 2026-10-01)

**1. Determinism, 2. Transparency, 3. Modularity, 4. Portability, 5. Performance.**

The lower number wins a tie. Never a list an agent may draft or amend.

- **Determinism**: the same Intents and seed give the same Events and state on
  every platform, so a log replays exactly. Test: `crates/aeolus/tests/fitness.rs`
  bans nondeterministic types in Aeolus; `crates/aeolus/tests/replay.rs` pins a
  golden Event stream and state hash.
- **Transparency**: a game on Anemoi can be seen working. Every stateful Boreas
  plugin declares an inspect dump, and `anemoi-sandbox` reads only those.
  Test: a script reproduces any bug headless.
- **Modularity**: any turn-based, top-down game can use the engine without
  inheriting another game's words or rules. Test: Aeolus has no Bevy dependency,
  and no crate names a game word (`crates/aeolus/tests/fitness.rs`).
- **Portability**: the engine runs natively and in the browser. Test: CI's wasm
  check on every crate.
- **Performance**: a frame and a Turn stay cheap as Units, routes and screens
  grow. Test: a `budget_ms` on the sandbox's walk script, so a slow frame fails
  with its number. Not yet set (`crates/anemoi-sandbox/scripts/walk.toml`).

## The named conflict (operator, 2026-10-01): determinism against performance

Speed wants hash maps, floats and clocks; replay forbids all three in Aeolus.
Determinism (1) outranks performance (5) when they tie: a faster rule ships
only if the replay test still holds, and an ordered structure is the default
even when an unordered one is quicker.
