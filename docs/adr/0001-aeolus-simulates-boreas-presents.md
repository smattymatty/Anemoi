# ADR 0001: Aeolus simulates, Boreas presents

**Status:** Accepted. Sealed by the operator as the engine half of Open Door
Dungeons' ADR 0004 (76b86b9, pacing c444a5a), moved here 2026-10-01 so the engine
carries its own reasons. Tier: timeless.

## Context

Anemoi is the engine for every turn-based, top-down game after its first. Its
shape had to be fixed before any game built movement on it, and a game's progress
must be replayable by a server that has no renderer.

**Architectural characteristics:** determinism, transparency, modularity,
portability (`docs/characteristics.md`).

**Options considered:**

1. Aeolus is the simulation; Boreas turns input into Intents and plays the Events
   Aeolus returns.
2. Bevy's ECS stays the truth; Aeolus offers helper functions.
3. Aeolus uses `bevy_ecs` standalone as its world.

## Decision

Option 1, with these parts:

- **Truth.** Aeolus owns the world as plain data: grid, Units, Turns. Boreas sends
  Intents and receives Events (moved, bumped, opened, refused). Boreas never
  decides a rule.
- **Composition.** The base is a **Unit**: name, health, cell, solidity, plus a
  slot `G` for the game's own data (`Unit<G>`). Aeolus calls the game through the
  `Rules` hooks.
- **Home.** This repository, public and open source. Each game pins a git tag; a
  fix ships as a new tag, taken when the game is ready. Local work overrides the
  dependency with a path.
- **Determinism.** Aeolus uses integers only, its own seeded RNG, ordered maps and
  no clock, so the same Intents and seed give the same Events everywhere.
- **Time.** Every action that costs a Turn advances the world. How a game orders
  Units in an Encounter, and any extra actions, is the game's data, not engine code.
- **Pacing.** Aeolus has no notion of animation time. Boreas overlaps a Turn's
  Events while Exploring and plays them one Unit at a time in an Encounter, and
  owns the input lock. While Exploring the lock does not wait for a step to finish,
  so a held direction chains into one glide; it ends with a single bump. Travel
  stops when an Encounter starts and re-paths every step. An Encounter starting lets
  the step in flight land, then needs a fresh press. A press during others' turns
  fast-forwards their animation and is not queued.

**Justification:**

1. A server replays Aeolus itself; nothing has to be extracted later.
2. Aeolus runs in tests, the sandbox, a server and any other engine; only Boreas is
   Bevy.
3. A typed slot plus hooks keeps every game's data compile-checked and
   serialisable, where a component bag would trade that for runtime lookups.
4. Pinned tags give "fix it once, every game gets it" without one fix breaking a
   game mid-build.

## Consequences

**Positive:** movement, gates and turn order are written once, for every game;
rules get pure unit tests; edited memory or saves cannot survive a replay.

**Negative:** a mirror layer in Boreas (every change of state needs an Event and a
presenter); two repositories to release, so an engine change needs a tag before a
game sees it.

**Accepted trade-offs:** integer-only rules over float convenience, for replayable
results; the cost of the Boreas mirror, for an engine-agnostic Aeolus.

## Governance

**Fitness functions** (`crates/aeolus/tests/`): Aeolus has no Bevy dependency and
nothing nondeterministic (`fitness.rs`); no crate names a game's words
(`fitness.rs`); the same Intents and seed give a golden Event stream and state hash,
natively and on wasm (`replay.rs`, CI).

**Manual review:** `/thermo-nuclear-code-quality-review` before an Aeolus or Boreas
change merges; each game checks the tag it pins at every engine release.
