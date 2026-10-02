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

**Exploring**:
The Mode where every waiting Intent's Events play together, a held direction
repeats, and a click Travels.

**Encounter**:
The Mode where each Unit's Intent plays in its own beat and every Step is one
press or one click.

**Travel**:
Walking to a chosen cell one Step at a time by the shortest route, chosen again
before every Step.

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

**Offer**:
One thing a Unit can do to one cell, a row of a Tile Menu: a Step, a Travel, one
of the game's own Intents, or Look. Next to the Unit the Step comes first, so a
click does what a direction key does (`aeolus::offers`; `Rules::offers` adds the
game's own).

**Tile Menu**:
The Offers for one cell, opened by clicking it or from the tile cursor. Its first
Offer is what a Step or Travel there does; Look is always last.

**Look**:
Reading what is on a cell. It costs no Turn, changes nothing and is never an
Intent, so no replay holds it.

**Refusal**:
Why a Unit cannot take an Offer: the missing Conditions of a gate, or no route. A
refused Offer stays visible with its reason; taking it sends no Intent. Not the
refused Event, which is a bump the world said no to.
