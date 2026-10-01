# Anemoi vocabulary

The engine speaks in neutral words. A game maps its own names onto these and
keeps them in its own code: no game's words appear in `crates/`
(`crates/aeolus/tests/fitness.rs` holds the deny-list).

**Unit**:
Anything on the grid that acts or blocks: name, health, cell, solid, plus `ext`,
the game's own data (`Unit<G>`).

**Intent**:
What a Unit asks to do: a `Step` in a direction, or one of the game's own.
Intents are the only input to the world once it is set up, so a log of them
replays it. Only setup and `Rules::act` change the world directly; Boreas holds
it read-only.

**Event**:
What happened because of an Intent: moved, bumped, opened, refused, or one of the
game's own. Boreas plays Events; it never decides them.

**Turn**:
The world's clock. An Intent that costs a Turn advances it; there is no other time.

**Gate**:
Terrain that opens for some Units: a list of routes.

**Route**:
One way through a gate: a list of conditions that must all hold.

**Condition**:
One fact a game checks about a Unit (`Rules::holds`), such as a thing it carries.

**Role**:
The part a Unit plays, as a game defines it. It lives in `ext`; the engine never
reads it, and a condition may test it.

**Rules**:
The trait a game implements: what terrain is, which conditions hold, what an opened
gate becomes, what bumping a blocking cell does, and how its own Intents play out.
