//! Play a game from a script, see it, and read its state. A runner is
//! `fn main() { anemoi_sandbox::run::<MyGame>() }`; `--window` watches the same script.
//! Headless by default at a fixed 60 fps clock, so runs repeat exactly. Writes
//! `target/sandbox/<script>/`: `frame_NNNN.png`, `state.log`, `fetches.log`.
//! A `golden` beat checks its frame against an approved image; `--bless` approves.
//! TODO: scripts start from the game's startup; with Turn Logs, from any recorded Turn.
#![cfg(not(target_arch = "wasm32"))]

use std::collections::{BTreeMap, HashMap};
use std::fs::{self, File};
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use aeolus::Cell;
use bevy::app::SubApps;
use bevy::asset::RenderAssetUsages;
use bevy::asset::io::file::FileAssetReader;
use bevy::asset::io::{
    AssetReader, AssetReaderError, AssetSource, AssetSourceBuilder, AssetSourceId, PathStream,
    Reader,
};
use bevy::camera::RenderTarget;
use bevy::diagnostic::{FrameCount, update_frame_count};
use bevy::input::InputSystems;
use bevy::prelude::*;
use bevy::render::RenderPlugin;
use bevy::render::render_resource::{
    Extent3d, PollType, TextureDimension, TextureFormat, TextureUsages,
};
use bevy::render::renderer::RenderDevice;
use bevy::render::view::screenshot::{Screenshot, save_to_disk};
use bevy::time::TimeUpdateStrategy;
use bevy::window::ExitCondition;
use bevy::winit::WinitPlugin;
use boreas::cursor::{self, Cursor, CursorPlugin};
use boreas::inspect::{self, InspectApp};
use boreas::intent::TravelTo;
use image::RgbaImage;
use serde::Deserialize;

/// What a game hands the runner.
pub trait Config: 'static {
    /// Every game plugin, without Bevy's `DefaultPlugins`.
    fn game() -> impl Plugin;

    /// The asset settings the game itself runs with.
    fn asset_plugin() -> AssetPlugin {
        AssetPlugin::default()
    }

    /// Headless runs have no window: tell the game the offscreen view's size.
    fn view_size(_app: &mut App, _size: UVec2) {}

    /// A scripted `click_tile`: injected past the pointer. By default, a Travel request.
    fn click_tile(world: &mut World, at: Cell) {
        world.write_message(TravelTo(at));
    }

    /// The world point a `cursor_at` cell aims at: its centre, cells 16 px wide.
    /// Fixed apart from the game's `Bindings`, so a wrong `cell_px` shows.
    fn cell_center(at: Cell) -> Vec2 {
        (Vec2::new(at.x as f32, at.y as f32) + 0.5) * 16.0
    }
}

const VIEW: UVec2 = UVec2::new(1280, 720);
/// Frames after the last scripted one, so screenshot readback lands on disk.
const TAIL: u32 = 3;
/// A golden pixel matches while every channel is within this: GPUs raster differently.
const CHANNEL_TOLERANCE: u8 = 24;
/// Pixels past the tolerance a frame may hold; a 1 px outline grown to 2 px is 52.
const MAX_DIFFERING: u32 = 16;

/// A script: how long to run, and what happens on which frame.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Script {
    frames: u32,
    /// Dump state every N frames as well as on every scripted frame.
    #[serde(default)]
    dump_every: Option<u32>,
    /// Wall-clock ceilings; loose on purpose, they catch regressions, not noise.
    #[serde(default)]
    budget_ms: Option<Budget>,
    #[serde(default)]
    at: Vec<Beat>,
}

#[derive(Deserialize, Clone, Copy)]
#[serde(deny_unknown_fields)]
struct Budget {
    startup: f64,
    max_frame: f64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Beat {
    frame: u32,
    /// Keys pressed this frame and held until released.
    #[serde(default)]
    press: Vec<String>,
    #[serde(default)]
    release: Vec<String>,
    #[serde(default)]
    mouse_press: bool,
    #[serde(default)]
    mouse_release: bool,
    /// The button `mouse_press` and `mouse_release` use.
    #[serde(default)]
    button: Button,
    /// Puts the pointer on a cell's screen position, where it stays.
    #[serde(default)]
    cursor_at: Option<[i32; 2]>,
    /// A Travel request straight to the game, skipping the pointer.
    #[serde(default)]
    click_tile: Option<[i32; 2]>,
    #[serde(default)]
    shot: bool,
    /// With `shot`: the approved image this frame must match, relative to the script.
    #[serde(default)]
    golden: Option<PathBuf>,
    /// `[x, y, w, h]`: the part of the frame the golden compares; all of it by default.
    #[serde(default)]
    region: Option<[u32; 4]>,
    /// A note copied into `state.log`, so a run reads as a story.
    #[serde(default)]
    note: Option<String>,
    /// Per dump: tokens its line must hold whole, e.g. `turn = "3"`.
    #[serde(default)]
    expect: HashMap<String, String>,
}

#[derive(Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Button {
    #[default]
    Left,
    Right,
}

impl From<Button> for MouseButton {
    fn from(b: Button) -> Self {
        match b {
            Button::Left => MouseButton::Left,
            Button::Right => MouseButton::Right,
        }
    }
}

/// The script, indexed by frame once.
#[derive(Resource)]
struct Plan {
    frames: u32,
    dump_every: Option<u32>,
    budget: Option<Budget>,
    beats: BTreeMap<u32, Vec<Beat>>,
}

impl Plan {
    /// Every golden beat, its image resolved against the script's folder.
    fn goldens(&self, dir: &Path) -> Vec<Golden> {
        let beats = self
            .beats
            .iter()
            .flat_map(|(f, bs)| bs.iter().map(move |b| (*f, b)));
        beats
            .filter_map(|(frame, b)| {
                let path = dir.join(b.golden.as_ref()?);
                Some(Golden {
                    frame,
                    path,
                    region: b.region,
                })
            })
            .collect()
    }
}

/// A frame and the approved image it must match.
struct Golden {
    frame: u32,
    path: PathBuf,
    region: Option<[u32; 4]>,
}

/// Expectations that did not hold, reported at exit.
#[derive(Resource, Default)]
struct Misses(Vec<String>);

#[derive(Resource)]
struct Out(PathBuf);

#[derive(Resource)]
struct Target(Handle<Image>);

/// Every asset read: path and size. What a launch would fetch from S3.
#[derive(Resource, Clone, Default)]
struct Fetches(Arc<Mutex<Vec<(PathBuf, u64)>>>);

/// The game's own file reader, logging each read into `Fetches`.
struct Counting {
    inner: FileAssetReader,
    root: PathBuf,
    log: Fetches,
}

impl AssetReader for Counting {
    async fn read<'a>(&'a self, path: &'a Path) -> Result<impl Reader + 'a, AssetReaderError> {
        let reader = self.inner.read(path).await?;
        let bytes = fs::metadata(self.root.join(path)).map_or(0, |m| m.len());
        self.log.0.lock().unwrap().push((path.to_owned(), bytes));
        Ok(reader)
    }
    async fn read_meta<'a>(&'a self, path: &'a Path) -> Result<impl Reader + 'a, AssetReaderError> {
        self.inner.read_meta(path).await
    }
    async fn read_directory<'a>(
        &'a self,
        path: &'a Path,
    ) -> Result<Box<PathStream>, AssetReaderError> {
        self.inner.read_directory(path).await
    }
    async fn is_directory<'a>(&'a self, path: &'a Path) -> Result<bool, AssetReaderError> {
        self.inner.is_directory(path).await
    }
}

fn dump_fetches(w: &World) -> String {
    let log = w.resource::<Fetches>().0.lock().unwrap();
    let bytes: u64 = log.iter().map(|(_, b)| b).sum();
    format!("count={} bytes={bytes}", log.len())
}

/// Run the script named on the command line against `C`'s game; exits nonzero
/// on a missed expectation or a blown budget.
pub fn run<C: Config>() {
    let launched = Instant::now();
    let args: Vec<String> = std::env::args().skip(1).collect();
    let windowed = args.iter().any(|a| a == "--window");
    let blessing = args.iter().any(|a| a == "--bless");
    let Some(path) = args.iter().find(|a| !a.starts_with("--")) else {
        fail("usage: sandbox <script.toml> [--window | --bless]");
    };
    if windowed && blessing {
        fail("--bless runs headless: goldens are the offscreen view");
    }
    let plan = load(Path::new(path)).unwrap_or_else(|e| fail(&e));
    let goldens = plan.goldens(Path::new(path).parent().unwrap_or(Path::new("")));
    let name = Path::new(path)
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy();
    let out = PathBuf::from("target/sandbox").join(name.as_ref());
    let _ = fs::remove_dir_all(&out);
    fs::create_dir_all(&out).unwrap_or_else(|e| fail(&format!("{}: {e}", out.display())));
    let (frames, budget) = (plan.frames, plan.budget);
    let root = FileAssetReader::get_base_path().join("assets");
    if !root.is_dir() {
        fail(&format!(
            "no asset folder at {}; run through `cargo run`",
            root.display()
        ));
    }
    let fetches = Fetches::default();
    let log = fetches.clone();
    // Registered before `AssetPlugin`, so it becomes the default source.
    let source = AssetSourceBuilder::new(move || {
        Box::new(Counting {
            inner: FileAssetReader::new("assets"),
            root: root.clone(),
            log: log.clone(),
        })
    })
    .with_watcher(AssetSource::get_default_watcher(
        "assets".into(),
        Duration::from_millis(300),
    ));

    let mut app = App::new();
    app.register_asset_source(AssetSourceId::Default, source)
        .insert_resource(fetches.clone())
        .add_plugins(plugins::<C>(windowed))
        .add_plugins(C::game())
        .add_message::<TravelTo>()
        .insert_resource(plan)
        .insert_resource(Out(out.clone()))
        .init_resource::<Misses>()
        .add_systems(
            PreUpdate,
            (drive, point::<C>, click::<C>)
                .chain()
                .after(InputSystems)
                .after(cursor::feed),
        )
        // Before the counter ticks, so `dump` sees the frame `drive` saw.
        .add_systems(Last, dump.before(update_frame_count))
        .inspect("fetches", dump_fetches);
    if !app.is_plugin_added::<CursorPlugin>() {
        app.add_plugins(CursorPlugin);
    }

    let misses = if windowed {
        app.add_systems(Last, quit_after_script.after(dump));
        match app.run() {
            AppExit::Success => Vec::new(),
            AppExit::Error(_) => vec!["see state.log".into()],
        }
    } else {
        C::view_size(&mut app, VIEW);
        let mut misses = run_headless(app, frames, budget, &out, launched);
        if !blessing {
            misses.extend(goldens.iter().filter_map(|g| check(g, &out).err()));
        } else if misses.is_empty() {
            misses = goldens
                .iter()
                .filter_map(|g| bless(g, &out).err())
                .collect();
        } else {
            eprintln!("sandbox: nothing blessed: the run missed");
        }
        misses
    };
    write_fetches(&fetches, &out);
    println!("sandbox: wrote {}", out.display());
    if !misses.is_empty() {
        for miss in &misses {
            eprintln!("sandbox: FAIL {miss}");
        }
        std::process::exit(1);
    }
}

fn plugins<C: Config>(windowed: bool) -> impl PluginGroup {
    let base = DefaultPlugins
        .set(ImagePlugin::default_nearest())
        .set(C::asset_plugin());
    if windowed {
        return base.set(WindowPlugin::default()).build();
    }
    base.set(WindowPlugin {
        primary_window: None,
        exit_condition: ExitCondition::DontExit,
        ..default()
    })
    .set(RenderPlugin {
        synchronous_pipeline_compilation: true,
        ..default()
    })
    .build()
    .disable::<WinitPlugin>()
}

/// Our own loop: a fixed clock, and wait for the GPU each frame (Bevy's
/// `externally_driven_headless_renderer` example).
/// Startup: process start through frame 2, where the game first draws and its
/// pipelines compile (878 ms cold after a rebuild, ~30 ms warm, for one game).
const WARM: u32 = 2;

fn run_headless(
    mut app: App,
    frames: u32,
    budget: Option<Budget>,
    out: &Path,
    launched: Instant,
) -> Vec<String> {
    let image = offscreen_image(&mut app);
    app.insert_resource(Target(image))
        .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f64(
            1.0 / 60.0,
        )))
        .add_systems(Update, aim_camera);
    app.finish();
    app.cleanup();
    let mut apps: SubApps = std::mem::take(app.sub_apps_mut());
    let mut startup = Duration::ZERO;
    let mut slowest = (0, Duration::ZERO);
    for frame in 0..=frames + TAIL {
        let tick = Instant::now();
        apps.update();
        apps.main
            .world()
            .resource::<RenderDevice>()
            .wgpu_device()
            .poll(PollType::Wait {
                submission_index: None,
                timeout: None,
            })
            .unwrap_or_else(|e| fail(&format!("GPU poll: {e:?}")));
        if frame == WARM {
            startup = launched.elapsed();
        } else if frame > WARM && tick.elapsed() > slowest.1 {
            slowest = (frame, tick.elapsed());
        }
    }
    let ms = |d: Duration| d.as_secs_f64() * 1000.0;
    let total = launched.elapsed() - startup;
    let mean = ms(total) / f64::from(frames + TAIL - WARM);
    let line = format!(
        "timing: startup_ms={:.0} mean_ms={mean:.1} max_ms={:.1}@frame{}",
        ms(startup),
        ms(slowest.1),
        slowest.0
    );
    println!("sandbox: {line}");
    append(&out.join("state.log"), &line);
    let mut misses = std::mem::take(&mut apps.main.world_mut().resource_mut::<Misses>().0);
    if let Some(b) = budget {
        if ms(startup) > b.startup {
            misses.push(format!(
                "startup {:.0} ms over budget {}",
                ms(startup),
                b.startup
            ));
        }
        if ms(slowest.1) > b.max_frame {
            let over = format!("frame {} took {:.1} ms", slowest.0, ms(slowest.1));
            misses.push(format!("{over}, over budget {}", b.max_frame));
        }
    }
    misses
}

fn append(path: &Path, line: &str) {
    File::options()
        .create(true)
        .append(true)
        .open(path)
        .and_then(|mut f| writeln!(f, "{line}"))
        .unwrap_or_else(|e| fail(&format!("{}: {e}", path.display())));
}

/// One line per asset read, in order: the launch's fetch list.
fn write_fetches(fetches: &Fetches, out: &Path) {
    for (path, bytes) in fetches.0.lock().unwrap().iter() {
        append(
            &out.join("fetches.log"),
            &format!("{bytes:>8} {}", path.display()),
        );
    }
}

fn offscreen_image(app: &mut App) -> Handle<Image> {
    let mut image = Image::new_uninit(
        Extent3d {
            width: VIEW.x,
            height: VIEW.y,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD,
    );
    image.texture_descriptor.usage |= TextureUsages::RENDER_ATTACHMENT;
    app.world_mut().resource_mut::<Assets<Image>>().add(image)
}

/// The game spawns its own camera; headless, point it at the offscreen image.
/// With no window there is no default UI camera, so it draws the UI too.
fn aim_camera(mut commands: Commands, cams: Query<Entity, Added<Camera2d>>, target: Res<Target>) {
    for cam in &cams {
        commands
            .entity(cam)
            .insert((RenderTarget::from(target.0.clone()), IsDefaultUiCamera));
    }
}

/// Apply this frame's beats: keys, and a screenshot.
fn drive(
    mut commands: Commands,
    frame: Res<FrameCount>,
    plan: Res<Plan>,
    out: Res<Out>,
    target: Option<Res<Target>>,
    mut keys: ResMut<ButtonInput<KeyCode>>,
    mut mouse: ResMut<ButtonInput<MouseButton>>,
) {
    for beat in plan.beats.get(&frame.0).into_iter().flatten() {
        for key in &beat.press {
            keys.press(key_code(key).unwrap_or_else(|e| fail(&e)));
        }
        for key in &beat.release {
            keys.release(key_code(key).unwrap_or_else(|e| fail(&e)));
        }
        if beat.mouse_press {
            mouse.press(beat.button.into());
        }
        if beat.mouse_release {
            mouse.release(beat.button.into());
        }
        if beat.shot {
            let file = frame_file(&out.0, frame.0);
            let shot = match &target {
                Some(t) => Screenshot::image(t.0.clone()),
                None => Screenshot::primary_window(),
            };
            commands.spawn(shot).observe(save_to_disk(file));
        }
    }
}

fn frame_file(out: &Path, frame: u32) -> PathBuf {
    out.join(format!("frame_{frame:04}.png"))
}

/// The saved frame against its golden; a miss names the frame and the count.
fn check(g: &Golden, out: &Path) -> Result<(), String> {
    let at = format!("frame {}: golden {}", g.frame, g.path.display());
    let file = frame_file(out, g.frame);
    let got = decode(&file).map_err(|e| format!("{at}: {}: {e}", file.display()))?;
    let want = decode(&g.path).map_err(|e| format!("{at}: {e}; approve with --bless"))?;
    let n = differing(&got, &want, g.region).map_err(|e| format!("{at}: {e}"))?;
    if n > MAX_DIFFERING {
        return Err(format!(
            "{at}: {n} pixels differ (max {MAX_DIFFERING}, channel tolerance {CHANNEL_TOLERANCE})"
        ));
    }
    Ok(())
}

/// Approves the saved frame: copies it over the golden, byte for byte.
fn bless(g: &Golden, out: &Path) -> Result<(), String> {
    let from = frame_file(out, g.frame);
    let at = format!("frame {}: golden {}", g.frame, g.path.display());
    if let Some(dir) = g.path.parent() {
        fs::create_dir_all(dir).map_err(|e| format!("{at}: {e}"))?;
    }
    fs::copy(&from, &g.path).map_err(|e| format!("{at}: {}: {e}", from.display()))?;
    println!("sandbox: blessed {}", g.path.display());
    Ok(())
}

fn decode(path: &Path) -> Result<RgbaImage, String> {
    image::open(path)
        .map(|i| i.to_rgba8())
        .map_err(|e| e.to_string())
}

/// Pixels in `region` with any channel past `CHANNEL_TOLERANCE`.
fn differing(got: &RgbaImage, want: &RgbaImage, region: Option<[u32; 4]>) -> Result<u32, String> {
    let (w, h) = want.dimensions();
    if got.dimensions() != (w, h) {
        return Err(format!("frame is {:?}, golden {w}x{h}", got.dimensions()));
    }
    let [x, y, rw, rh] = region.unwrap_or([0, 0, w, h]);
    if x.checked_add(rw).is_none_or(|r| r > w) || y.checked_add(rh).is_none_or(|b| b > h) {
        return Err(format!(
            "region {:?} leaves the {w}x{h} frame",
            [x, y, rw, rh]
        ));
    }
    let off = |px: u32, py: u32| {
        let (a, b) = (got.get_pixel(px, py).0, want.get_pixel(px, py).0);
        a.iter()
            .zip(b)
            .any(|(a, b)| a.abs_diff(b) > CHANNEL_TOLERANCE)
    };
    let cells = (y..y + rh).flat_map(|py| (x..x + rw).map(move |px| (px, py)));
    Ok(cells.filter(|&(px, py)| off(px, py)).count() as u32)
}

/// Aims the pointer at this frame's `cursor_at` cell through the camera, and
/// holds it there after the window's feed, as a resting mouse would.
fn point<C: Config>(
    frame: Res<FrameCount>,
    plan: Res<Plan>,
    camera: Query<(&Camera, &GlobalTransform), With<Camera2d>>,
    mut held: Local<Option<Vec2>>,
    mut cursor: ResMut<Cursor>,
    mut misses: ResMut<Misses>,
) {
    let beats = plan.beats.get(&frame.0).into_iter().flatten();
    for [x, y] in beats.filter_map(|b| b.cursor_at) {
        let world = C::cell_center(Cell::new(x, y)).extend(0.0);
        let screen = camera
            .single()
            .map_err(|e| e.to_string())
            .and_then(|(cam, at)| cam.world_to_viewport(at, world).map_err(|e| e.to_string()));
        match screen {
            Ok(p) => *held = Some(p),
            Err(e) => misses
                .0
                .push(format!("frame {}: cursor_at ({x},{y}): {e}", frame.0)),
        }
    }
    if held.is_some() {
        cursor.0 = *held;
    }
}

/// This frame's `click_tile`s, through the game's hook.
fn click<C: Config>(world: &mut World) {
    let frame = world.resource::<FrameCount>().0;
    let beats = world.resource::<Plan>().beats.get(&frame);
    let cells: Vec<Cell> = beats
        .into_iter()
        .flatten()
        .filter_map(|b| b.click_tile)
        .map(|[x, y]| Cell::new(x, y))
        .collect();
    for at in cells {
        C::click_tile(world, at);
    }
}

/// One `state.log` line per scripted frame (and every `dump_every`), from the
/// game's declared dumps only; checks this frame's expectations against them.
fn dump(world: &mut World) {
    let frame = world.resource::<FrameCount>().0;
    let plan = world.resource::<Plan>();
    let beats = plan.beats.get(&frame).map_or(&[][..], Vec::as_slice);
    let periodic = plan
        .dump_every
        .is_some_and(|n| n > 0 && frame.is_multiple_of(n));
    if beats.is_empty() && !periodic && frame != plan.frames {
        return;
    }
    let dumps = inspect::snapshot(world);
    let (line, misses) = report(frame, beats, &dumps);
    append(&world.resource::<Out>().0.join("state.log"), &line);
    world.resource_mut::<Misses>().0.extend(misses);
}

/// The frame's log line, and the expectations its dumps miss.
fn report(frame: u32, beats: &[Beat], dumps: &[(&str, String)]) -> (String, Vec<String>) {
    let mut line = format!("frame={frame}");
    let mut misses = Vec::new();
    for beat in beats {
        let keys = beat.press.iter().map(|k| format!("+{k}"));
        let keys: Vec<String> = keys
            .chain(beat.release.iter().map(|k| format!("-{k}")))
            .collect();
        if !keys.is_empty() {
            line += &format!(" keys=[{}]", keys.join(" "));
        }
        if let Some([x, y]) = beat.cursor_at {
            line += &format!(" cursor_at=({x},{y})");
        }
        if beat.mouse_press {
            line += &format!(" mouse=[+{:?}]", beat.button);
        }
        if beat.mouse_release {
            line += &format!(" mouse=[-{:?}]", beat.button);
        }
        if let Some([x, y]) = beat.click_tile {
            line += &format!(" click_tile=({x},{y})");
        }
        if let Some(note) = &beat.note {
            line += &format!(" note=\"{note}\"");
        }
        for (name, want) in &beat.expect {
            let Some((_, got)) = dumps.iter().find(|(n, _)| n == name) else {
                let known: Vec<_> = dumps.iter().map(|(n, _)| *n).collect();
                misses.push(format!("frame {frame}: no dump {name:?}; known: {known:?}"));
                continue;
            };
            let tokens: Vec<&str> = got.split_whitespace().collect();
            if !want.split_whitespace().all(|w| tokens.contains(&w)) {
                misses.push(format!(
                    "frame {frame}: {name} expected {want:?}, got {got:?}"
                ));
            }
        }
    }
    for (name, state) in dumps {
        line += &format!(" | {name}: {state}");
    }
    (line, misses)
}

/// Windowed runs end after the script, failing if any expectation missed.
fn quit_after_script(
    frame: Res<FrameCount>,
    plan: Res<Plan>,
    misses: Res<Misses>,
    mut exit: MessageWriter<AppExit>,
) {
    if frame.0 < plan.frames + TAIL {
        return;
    }
    for miss in &misses.0 {
        eprintln!("sandbox: FAIL {miss}");
    }
    exit.write(if misses.0.is_empty() {
        AppExit::Success
    } else {
        AppExit::error()
    });
}

fn load(path: &Path) -> Result<Plan, String> {
    let text = fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    parse(&text).map_err(|e| format!("{}: {e}", path.display()))
}

/// Parse and index a script; bad keys fail before the game starts.
fn parse(text: &str) -> Result<Plan, String> {
    let script: Script = toml::from_str(text).map_err(|e| e.to_string())?;
    let mut beats: BTreeMap<u32, Vec<Beat>> = BTreeMap::new();
    for beat in script.at {
        for key in beat.press.iter().chain(&beat.release) {
            key_code(key)?;
        }
        if beat.golden.is_some() && !beat.shot {
            return Err(format!("frame {}: a golden needs shot = true", beat.frame));
        }
        if beat.region.is_some() && beat.golden.is_none() {
            return Err(format!("frame {}: a region needs a golden", beat.frame));
        }
        beats.entry(beat.frame).or_default().push(beat);
    }
    Ok(Plan {
        frames: script.frames,
        dump_every: script.dump_every,
        budget: script.budget_ms,
        beats,
    })
}

/// Keys a script may name. Unknown names fail before the game starts.
fn key_code(name: &str) -> Result<KeyCode, String> {
    Ok(match name {
        "W" => KeyCode::KeyW,
        "A" => KeyCode::KeyA,
        "S" => KeyCode::KeyS,
        "D" => KeyCode::KeyD,
        "L" => KeyCode::KeyL,
        "V" => KeyCode::KeyV,
        "Up" => KeyCode::ArrowUp,
        "Down" => KeyCode::ArrowDown,
        "Left" => KeyCode::ArrowLeft,
        "Right" => KeyCode::ArrowRight,
        "Space" => KeyCode::Space,
        "Enter" => KeyCode::Enter,
        "Escape" => KeyCode::Escape,
        "0" => KeyCode::Digit0,
        "1" => KeyCode::Digit1,
        "2" => KeyCode::Digit2,
        "3" => KeyCode::Digit3,
        other => {
            return Err(format!(
                "unknown key {other:?}; known: W A S D L V Up Down Left Right Space Enter Escape 0 1 2 3"
            ));
        }
    })
}

fn fail(msg: &str) -> ! {
    eprintln!("sandbox: {msg}");
    std::process::exit(2);
}

#[cfg(test)]
mod tests {
    use super::*;

    const FULL: &str = r#"
        frames = 30
        dump_every = 10
        budget_ms = { startup = 8000, max_frame = 300 }
        [[at]]
        frame = 5
        press = ["W", "0", "1", "2", "3"]
        release = ["Up"]
        mouse_press = true
        mouse_release = true
        button = "Right"
        cursor_at = [1, -3]
        click_tile = [4, -2]
        shot = true
        golden = "golden/five.png"
        region = [1, 2, 3, 4]
        note = "hi"
        expect = { turn = "3" }
    "#;

    #[test]
    fn every_schema_field_parses() {
        let plan = parse(FULL).unwrap();
        assert_eq!((plan.frames, plan.dump_every), (30, Some(10)));
        let b = plan.budget.unwrap();
        assert_eq!((b.startup, b.max_frame), (8000.0, 300.0));
        let beat = &plan.beats[&5][0];
        assert_eq!(beat.click_tile, Some([4, -2]));
        assert_eq!(
            (beat.cursor_at, beat.button),
            (Some([1, -3]), Button::Right)
        );
        let left = parse("frames = 1\n[[at]]\nframe = 1\nmouse_press = true").unwrap();
        assert_eq!(left.beats[&1][0].button, Button::Left, "Left by default");
        assert_eq!(MouseButton::from(Button::Right), MouseButton::Right);
        assert!(beat.mouse_press && beat.mouse_release && beat.shot);
        assert_eq!(beat.expect["turn"], "3");
        let goldens = plan.goldens(Path::new("scripts"));
        assert_eq!(goldens[0].path, Path::new("scripts/golden/five.png"));
        assert_eq!(
            (goldens[0].frame, goldens[0].region),
            (5, Some([1, 2, 3, 4]))
        );
    }

    #[test]
    fn a_golden_needs_a_shot_and_a_region_needs_a_golden() {
        let shotless = parse("frames = 1\n[[at]]\nframe = 1\ngolden = \"g.png\"");
        assert!(shotless.err().unwrap().contains("needs shot"));
        let bare = parse("frames = 1\n[[at]]\nframe = 1\nshot = true\nregion = [0, 0, 1, 1]");
        assert!(bare.err().unwrap().contains("needs a golden"));
    }

    fn grey(v: u8) -> RgbaImage {
        RgbaImage::from_pixel(4, 3, image::Rgba([v, v, v, 255]))
    }

    #[test]
    fn a_pixel_differs_only_past_the_channel_tolerance_and_inside_the_region() {
        let want = grey(100);
        let mut got = grey(100 + CHANNEL_TOLERANCE);
        assert_eq!(
            differing(&got, &want, None),
            Ok(0),
            "at the tolerance: alike"
        );
        got.put_pixel(3, 2, image::Rgba([100, 100, 101 + CHANNEL_TOLERANCE, 255]));
        assert_eq!(differing(&got, &want, None), Ok(1), "one channel past it");
        assert_eq!(
            differing(&got, &want, Some([0, 0, 3, 3])),
            Ok(0),
            "outside the region"
        );
        assert_eq!(
            differing(&got, &want, Some([3, 2, 1, 1])),
            Ok(1),
            "the region's last pixel"
        );
        assert!(
            differing(&got, &want, Some([3, 2, 2, 1])).is_err(),
            "region past the edge"
        );
        assert!(
            differing(&RgbaImage::new(3, 3), &want, None).is_err(),
            "sizes differ"
        );
    }

    #[test]
    fn a_golden_misses_by_frame_and_count_and_bless_approves_the_frame() {
        let out = std::env::temp_dir().join(format!("anemoi-golden-{}", std::process::id()));
        let _ = fs::remove_dir_all(&out);
        let g = Golden {
            frame: 7,
            path: out.join("golden/seven.png"),
            region: None,
        };
        let missing = check(&g, &out).unwrap_err();
        assert!(missing.starts_with("frame 7: golden ") && missing.contains("frame_0007.png"));
        assert!(
            bless(&g, &out).unwrap_err().starts_with("frame 7: "),
            "no frame, no bless"
        );
        let saved = |v: u8| {
            let img = RgbaImage::from_pixel(40, 30, image::Rgba([v, v, v, 255]));
            img.save(frame_file(&out, 7)).unwrap();
        };
        saved(10);
        assert!(
            check(&g, &out).unwrap_err().contains("--bless"),
            "no golden yet"
        );
        bless(&g, &out).unwrap();
        assert_eq!(
            fs::read(&g.path).unwrap(),
            fs::read(frame_file(&out, 7)).unwrap()
        );
        assert_eq!(check(&g, &out), Ok(()));
        saved(200);
        let miss = check(&g, &out).unwrap_err();
        assert!(miss.starts_with("frame 7: golden "), "{miss}");
        assert!(
            miss.ends_with(&format!(
                ": 1200 pixels differ (max {MAX_DIFFERING}, channel tolerance {CHANNEL_TOLERANCE})"
            )),
            "{miss}"
        );
        let _ = fs::remove_dir_all(&out);
    }

    /// `MAX_DIFFERING` is a ceiling: that many pixels off still passes, one more misses.
    #[test]
    fn a_golden_passes_at_the_max_differing_and_misses_one_past_it() {
        let out = std::env::temp_dir().join(format!("anemoi-golden-max-{}", std::process::id()));
        let _ = fs::remove_dir_all(&out);
        fs::create_dir_all(&out).unwrap();
        let g = Golden {
            frame: 3,
            path: out.join("max.png"),
            region: None,
        };
        let want = RgbaImage::from_pixel(40, 30, image::Rgba([10, 10, 10, 255]));
        want.save(&g.path).unwrap();
        let off = |n: u32| {
            let mut got = want.clone();
            for x in 0..n {
                got.put_pixel(x, 0, image::Rgba([200, 10, 10, 255]));
            }
            got.save(frame_file(&out, 3)).unwrap();
            check(&g, &out)
        };
        assert_eq!(off(MAX_DIFFERING), Ok(()), "at the ceiling");
        let miss = off(MAX_DIFFERING + 1).unwrap_err();
        let n = MAX_DIFFERING + 1;
        assert!(miss.contains(&format!(": {n} pixels differ")), "{miss}");
        let _ = fs::remove_dir_all(&out);
    }

    /// A region whose far edge overflows `u32` is a miss, never a panic.
    #[test]
    fn a_region_past_u32_is_a_miss_not_a_panic() {
        let img = grey(0);
        assert!(differing(&img, &img, Some([u32::MAX, 0, 2, 1])).is_err());
        assert!(differing(&img, &img, Some([0, u32::MAX, 1, 2])).is_err());
    }

    #[test]
    fn unknown_fields_fail_at_both_levels() {
        assert!(parse("frames = 1\nspeed = 2").is_err());
        assert!(parse("frames = 1\n[[at]]\nframe = 1\nclick = [1, 1]").is_err());
        assert!(parse("frames = 1\n[[at]]\nframe = 1\nbutton = \"Middle\"").is_err());
        assert!(parse("frames = 1\nbudget_ms = { startup = 1, max_frame = 1, mean = 1 }").is_err());
    }

    #[test]
    fn digit_keys_map_and_unknown_keys_fail() {
        assert_eq!(key_code("L"), Ok(KeyCode::KeyL));
        assert_eq!(key_code("V"), Ok(KeyCode::KeyV));
        let digits = ["0", "1", "2", "3"].map(|k| key_code(k).unwrap());
        let want = [
            KeyCode::Digit0,
            KeyCode::Digit1,
            KeyCode::Digit2,
            KeyCode::Digit3,
        ];
        assert_eq!(digits, want);
        assert!(parse("frames = 1\n[[at]]\nframe = 1\nrelease = [\"4\"]").is_err());
    }

    #[test]
    fn report_logs_the_beat_and_checks_whole_tokens() {
        let plan = parse(FULL).unwrap();
        let dumps = [("turn", "13".to_string())];
        let (line, misses) = report(5, &plan.beats[&5], &dumps);
        assert!(
            line.starts_with("frame=5 keys=[+W +0 +1 +2 +3 -Up]"),
            "{line}"
        );
        assert!(
            line.contains(" cursor_at=(1,-3) mouse=[+Right] mouse=[-Right] click_tile=(4,-2) note=\"hi\" | turn: 13"),
            "{line}"
        );
        assert_eq!(misses.len(), 1, "\"3\" is not a whole token of \"13\"");
        let (_, misses) = report(5, &plan.beats[&5], &[("turn", "3 x".into())]);
        assert!(misses.is_empty());
        let (_, misses) = report(5, &plan.beats[&5], &[]);
        assert!(misses[0].contains("no dump \"turn\""), "{misses:?}");
    }
}
