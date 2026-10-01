# Boreas

Boreas is the Bevy front end for Aeolus: input to Intents, pacing the Events
that come back, tweens, menus and the inspect seam. It never decides a rule.

## Turns

Put your world in a `pace::Sim::new(world, player)` resource and add
`intent::IntentPlugin::<MyRules>::default()` (it brings `pace::PacePlugin`).

- `intent`: `Bindings` (remappable) turn keys into a `Step` for the player Unit.
  The game sets `Mode`: while `Exploring` a held key repeats and a click Travels
  by `aeolus::next_step`, a Step per `pace::Cadence` (default zero: every unlocked
  frame); the first press steps at once. A bump or refusal ends the Run: that key
  waits for release, another key steps at once. In an `Encounter` every Step is one
  press or one click. `TravelTo(cell)` is the click as a message.
- `pace`: every `Act { by, intent }` (input's, or the game's own) is applied to the
  world; its Events are triggered as `Play { by, event }` for your observers to
  present. Exploring plays them all at once; an Encounter plays one Unit's Intent
  per beat. `Lock` holds input while Events wait or any `Busy` runs; in an
  Encounter, any `Tween` too, so a Run glides while Exploring. Its `sim` dump
  (`turn=N player=(x,y)`) shows every game's world to the sandbox.
- `tween`: presentation-only `Transform` tweens, with an optional second leg.
- `inspect`: `app.inspect(name, dump)` declares a read-only state line;
  `inspect::snapshot` reads them all. `anemoi-sandbox` reads only these.

## UI

Add `UiPlugin` once. Build menus from
`ui::overlay`, `panel`, `text`, `button`, and `corner_button` using a `UiTheme`.
Attach `MenuSelection { selected: 0, count }` to the direct parent of a group of
numbered `Focusable` buttons. Keyboard focus supports Up/Down, W/S, and Tab;
Enter/Space activates. Pointer hover selects and pointer
press activates. `StyledButton` gets visible focus, hover, and pressed colors.

Read `UiActivated` messages in your game and map each entity to a game action.
Boreas never knows game-specific labels or actions. Add `UiSounds` with your
`focus` and `activate` audio handles to get the same feedback from keyboard and
pointer input. It is optional; the UI works silently without it. `UiFeedbackStats`
counts sounds played and can be exposed by a game's inspect seam.

```rust
app.add_plugins(boreas::ui::UiPlugin);
commands.insert_resource(boreas::ui::UiSounds {
    focus: assets.load("sounds/menu_move.ogg"),
    activate: assets.load("sounds/menu_click.ogg"),
    volume: 0.55,
});
```

Colours come from your game's Aeolus `Palette`. `boreas::palette` converts an
`Rgb8` or a palette index to a Bevy `Color`. Build a theme once with
`UiTheme::from_palette(&palette, ThemeIndices { .. })` and insert it as a resource;
the default theme is neutral grey. The game supplies its own action mapping,
screen state, and placement.
