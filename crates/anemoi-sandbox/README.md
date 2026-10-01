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

`cargo run --example sandbox -- sandbox/walk.toml [--window]` writes
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
press = ["W"]          # W A S D Up Down Left Right Space Enter Escape 0 1 2 3
release = []
cursor_at = [2, 3]     # pointer onto a cell through the camera; it rests there
mouse_press = false
mouse_release = false
button = "Left"        # or "Right": the button this beat's press/release uses
click_tile = [2, 3]    # Travel injected past the pointer, via Config::click_tile
shot = true
note = "copied into state.log"
expect = { turn = "3" }  # per dump: tokens its line must hold whole
```

`examples/walk.rs` is the smallest game it drives:
`cargo run -p anemoi-sandbox --example walk -- crates/anemoi-sandbox/scripts/walk.toml`;
`scripts/pointer.toml` clicks a cell through the real pointer path.
