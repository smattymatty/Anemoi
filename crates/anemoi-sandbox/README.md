# anemoi-sandbox

Play a Boreas game from a TOML script, headless or in a window, and read its
state. A separate crate, so games that only use Boreas' UI never compile it.

Implement `Config` for your game and call `run` from an example:

```rust,ignore
struct MyGame;

impl anemoi_sandbox::Config for MyGame {
    fn game() -> impl Plugin { my_game::GamePlugin }
    fn asset_plugin() -> AssetPlugin { my_game::asset_plugin() }
    fn view_size(app: &mut App, size: UVec2) { app.insert_resource(my_game::ViewSize(size)); }
    // `click_tile` defaults to a Travel request (`boreas::intent::TravelTo`).
    // `cell_center` defaults to a cell's centre at 16 world px, for `cursor_at`.
}

fn main() { anemoi_sandbox::run::<MyGame>() }
```

`cargo run --example sandbox -- sandbox/walk.toml [--window | --bless]` writes
`target/sandbox/walk/`: `frame_NNNN.png`, `state.log` and `fetches.log`. Headless
runs use a fixed 60 fps clock, so they repeat exactly. The runner reads only the
dumps plugins declare through `boreas::inspect`.

A script (unknown fields fail before the game starts):

```toml
frames = 40
dump_every = 10                                  # optional
budget_ms = { startup = 8000, max_frame = 300 }  # optional wall-clock ceilings

[[at]]
frame = 5
press = ["W"]          # W A S D L V Up Down Left Right Space Enter Escape = - 0 1 2 3
release = []
cursor_at = [2, 3]     # pointer onto a cell through the camera; it rests there
mouse_press = false
mouse_release = false
button = "Left"        # or "Right": the button this beat's press/release uses
scroll = 1.0           # wheel lines this frame; up is positive
click_tile = [2, 3]    # Travel injected past the pointer, via Config::click_tile
shot = true
golden = "golden/walk.png"  # with shot: the approved frame, relative to the script
region = [0, 0, 640, 360]   # optional [x, y, w, h] the golden compares
note = "copied into state.log"
expect = { turn = "3" }  # per dump: tokens its line must hold whole
```

A golden beat compares its saved frame after the run: a pixel differs when any
channel is more than 24 off, and more than 16 differing pixels is a miss naming
the frame and the count (a 1 px outline grown to 2 px is 52). `--bless` copies
the run's frames over its goldens, headless only, and blesses nothing if any
`expect` missed. Goldens are blessed on a desktop GPU; CI renders on lavapipe and
keeps `target/sandbox/` when a run fails. A lavapipe miss means re-blessing there,
never a looser tolerance.

`examples/walk.rs` is the smallest game it drives, with a wall line at x=6
and a shut gate at (6,4) whose Condition the walker lacks:
`cargo run -p anemoi-sandbox --example walk -- crates/anemoi-sandbox/scripts/walk.toml`;
`scripts/pointer.toml` clicks a cell through the real pointer path;
`scripts/hover.toml` aims at cells on and off the grid and shoots the outline;
`scripts/golden/` holds the approved frames: the hover outline, the open Tile Menu
with its focused row lit, the open menu with the pointer on another cell (no
second outline: the map is locked), a refused toast, and the lit walker;
`scripts/tile_menu.toml` (`1` hands clicks to the Tile Menu) opens, runs, Looks and
closes it, and takes refused rows past the wall ("no route") and at the gate
("locked"): a toast, no Turn; `scripts/tile_cursor.toml` drives the tile cursor (`L`).
The example starts all lit (vision's dev switch), so those frames show no darkness;
`scripts/vision.toml` presses `V` and shoots the radius-4 light falling off from
(4,2), the wall line lit from its near side and the column behind it dark.
`scripts/dark_cells.toml` shows dark cells answer nothing: a Travel click past the
light, a Tile Menu click behind the wall, and the tile cursor held at the light's edge.
