//! Fitness functions for the whole engine (ADR 0004, Governance): Aeolus stays
//! deterministic and Bevy-free, and neither crate speaks a game's words.
#![cfg(not(target_arch = "wasm32"))]

use std::fs;
use std::path::{Path, PathBuf};

/// Each breaks replay. Comments count too; the allowlist is empty on purpose.
const NONDETERMINISTIC: [&str; 7] = [
    "f32",
    "f64",
    "HashMap",
    "HashSet",
    "std::time",
    "Instant",
    "thread_rng",
];

/// Game words that never belong in the engine. Case-sensitive, and a word
/// counts unless a letter, digit or `_` comes right before it.
const GAME_WORDS: [&str; 25] = [
    "ODD",
    "Open Door",
    "Door",
    "door",
    "Barrier",
    "barrier",
    "Class",
    "Mercenary",
    "mercenary",
    "Rogue",
    "Mage",
    "Warrior",
    "Dungeon",
    "dungeon",
    "Floor",
    "Overworld",
    "Millhaven",
    "Quest",
    "Actor",
    "Gravestone",
    "Hitpoints",
    "Loadout",
    "Sentry",
    "Robotnik",
    "Ring Racers",
];

/// The neutral terms `CONTEXT.md` must define.
const VOCABULARY: [&str; 16] = [
    "Unit",
    "Intent",
    "Event",
    "Turn",
    "Exploring",
    "Encounter",
    "Travel",
    "Gate",
    "Route",
    "Condition",
    "Role",
    "Rules",
    "Offer",
    "Tile Menu",
    "Look",
    "Refusal",
];

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// Every text file under `dir`, sorted so failures read the same each run.
fn files(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        for entry in fs::read_dir(&d).expect("readable dir") {
            let path = entry.unwrap().path();
            let ext = path.extension().and_then(|e| e.to_str());
            if path.is_dir() {
                stack.push(path);
            } else if matches!(ext, Some("rs" | "md" | "toml" | "gpl")) {
                out.push(path);
            }
        }
    }
    out.sort();
    out
}

fn names(text: &str, word: &str) -> bool {
    text.match_indices(word).any(|(i, _)| {
        let before = text[..i].chars().next_back();
        !before.is_some_and(|c| c.is_alphanumeric() || c == '_')
    })
}

#[test]
fn aeolus_src_has_nothing_nondeterministic() {
    let src = root().join("crates/aeolus/src");
    let found = files(&src);
    assert!(found.len() >= 7, "aeolus sources not found: {found:?}");
    let mut hits = Vec::new();
    for path in found {
        let text = fs::read_to_string(&path).unwrap();
        for word in NONDETERMINISTIC.iter().filter(|w| text.contains(**w)) {
            hits.push((path.strip_prefix(&src).unwrap().to_owned(), *word));
        }
    }
    assert!(hits.is_empty(), "nondeterminism in aeolus: {hits:?}");
}

/// Lines naming Bevy inside any dependency table, target-specific ones too.
fn bevy_deps(manifest: &str) -> Vec<&str> {
    let mut table = "";
    let mut hits = Vec::new();
    for line in manifest.lines().map(str::trim) {
        if line.starts_with('[') {
            table = line;
        }
        if table.contains("dependencies") && line.to_lowercase().contains("bevy") {
            hits.push(line);
        }
    }
    hits
}

#[test]
fn aeolus_depends_on_no_bevy() {
    let manifest = fs::read_to_string(root().join("crates/aeolus/Cargo.toml")).unwrap();
    let hits = bevy_deps(&manifest);
    assert!(hits.is_empty(), "aeolus must not depend on Bevy: {hits:?}");
}

#[test]
fn bevy_counts_only_in_dependency_tables() {
    let manifest = "[package]\ndescription = \"Bevy-free\"\n\
        [dependencies]\nbevy = \"0.19\"\n\
        [target.'cfg(unix)'.dev-dependencies.bevy_ecs]\nversion = \"0.19\"\n";
    let want = [
        "bevy = \"0.19\"",
        "[target.'cfg(unix)'.dev-dependencies.bevy_ecs]",
    ];
    assert_eq!(bevy_deps(manifest), want);
}

#[test]
fn the_engine_speaks_no_game_words() {
    let root = root();
    let mut scanned = files(&root.join("crates"));
    scanned.extend(["README.md", "CONTEXT.md"].map(|f| root.join(f)));
    for deep in [
        "crates/boreas/src/ui/look.rs",
        "crates/aeolus/tests/data/test.gpl",
    ] {
        assert!(scanned.contains(&root.join(deep)), "not scanned: {deep}");
    }
    let mut hits = Vec::new();
    for path in scanned {
        let rel = path.strip_prefix(&root).unwrap().to_owned();
        // This file holds the deny-list itself.
        if rel == Path::new("crates/aeolus/tests/fitness.rs") {
            continue;
        }
        let text = fs::read_to_string(&path).unwrap();
        for word in GAME_WORDS.iter().filter(|w| names(&text, w)) {
            hits.push((rel.clone(), *word));
        }
    }
    assert!(hits.is_empty(), "game words in the engine: {hits:?}");
}

#[test]
fn context_defines_the_neutral_vocabulary() {
    let text = fs::read_to_string(root().join("CONTEXT.md")).unwrap();
    let missing: Vec<_> = VOCABULARY
        .iter()
        .filter(|t| !text.contains(&format!("**{t}**:")))
        .collect();
    assert!(missing.is_empty(), "CONTEXT.md lacks: {missing:?}");
}

#[test]
fn a_game_word_counts_unless_it_continues_another_word() {
    assert!(names("a Door here", "Door"));
    assert!(names("ODDs", "ODD"));
    assert!(names("(Mage)", "Mage"));
    assert!(!names("Image", "Mage"));
    assert!(!names("indoor", "door"));
    assert!(!names("div_floor", "floor"));
}

/// Boreas files whose plugin declares no dump yet. Shrink-only; empty on purpose.
const UNDUMPED: [&str; 0] = [];

/// A plugin impl with no `.inspect(` dump, both read above the file's tests.
fn undumped_plugin(text: &str) -> Option<bool> {
    let shipped = text.split("#[cfg(test)]").next().unwrap_or_default();
    let plugin = shipped
        .lines()
        .map(str::trim)
        .any(|l| l.starts_with("impl") && l.contains(" Plugin for "));
    plugin.then(|| !shipped.contains(".inspect("))
}

#[test]
fn every_boreas_plugin_declares_a_dump() {
    let src = root().join("crates/boreas/src");
    let (mut plugins, mut hits) = (0, Vec::new());
    for path in files(&src) {
        let rel = path
            .strip_prefix(&src)
            .unwrap()
            .to_string_lossy()
            .into_owned();
        match undumped_plugin(&fs::read_to_string(&path).unwrap()) {
            Some(true) => hits.push(rel),
            Some(false) => plugins += 1,
            None => {}
        }
    }
    assert!(plugins + hits.len() >= 4, "boreas plugins not found");
    let new: Vec<_> = hits
        .iter()
        .filter(|h| !UNDUMPED.contains(&h.as_str()))
        .collect();
    assert!(new.is_empty(), "plugins without a dump: {new:?}");
    let fixed: Vec<_> = UNDUMPED
        .iter()
        .filter(|u| !hits.iter().any(|h| h == *u))
        .collect();
    assert!(
        fixed.is_empty(),
        "dumped now, drop from UNDUMPED: {fixed:?}"
    );
}

#[test]
fn a_dump_counts_only_above_the_tests() {
    let generic = "impl<G: Game> Plugin for P<G> {}\n";
    assert_eq!(undumped_plugin(generic), Some(true));
    assert_eq!(
        undumped_plugin(&format!("{generic}app.inspect(\"p\", d);")),
        Some(false)
    );
    let test_only = format!("{generic}#[cfg(test)]\nmod t {{ app.inspect(\"p\", d); }}");
    assert_eq!(undumped_plugin(&test_only), Some(true));
    assert_eq!(undumped_plugin("#[cfg(test)]\nimpl Plugin for T {}"), None);
}

/// Colour constructors a Boreas widget never calls: every colour is a `UiTheme`
/// role, so one game's look is one theme (operator, compound run 5).
const RAW_COLOUR: [&str; 7] = [
    "Color::srgb",
    "Color::hsl",
    "Color::linear",
    "Color::oklch",
    "Color::WHITE",
    "Color::BLACK",
    "Srgba::",
];

/// Numbered lines of a file's shipped code that `hit`s, skipping the `impl Default
/// for <owner>` block: the one place its values are made.
fn shipped_hits(text: &str, owner: &str, hit: impl Fn(&str) -> bool) -> Vec<usize> {
    let shipped = text.split("#[cfg(test)]").next().unwrap_or_default();
    let header = format!("impl Default for {owner}");
    let mut in_default = false;
    let mut hits = Vec::new();
    for (i, line) in shipped.lines().enumerate() {
        if line.starts_with(&header) {
            in_default = true;
        } else if in_default && line == "}" {
            in_default = false;
        } else if !in_default && hit(line) {
            hits.push(i + 1);
        }
    }
    hits
}

/// Raw colours in a Boreas file's shipped code. The palette adapter and the neutral
/// default theme are where colours are made; everything else takes a role.
fn raw_colours(rel: &str, text: &str) -> Vec<usize> {
    if rel == "palette.rs" {
        return Vec::new();
    }
    shipped_hits(text, "UiTheme", |l| {
        RAW_COLOUR.iter().any(|c| l.contains(c))
    })
}

#[test]
fn boreas_widgets_take_colours_from_the_theme() {
    let src = root().join("crates/boreas/src");
    let mut hits = Vec::new();
    for path in files(&src) {
        let rel = path
            .strip_prefix(&src)
            .unwrap()
            .to_string_lossy()
            .into_owned();
        for line in raw_colours(&rel, &fs::read_to_string(&path).unwrap()) {
            hits.push(format!("{rel}:{line}"));
        }
    }
    assert!(hits.is_empty(), "raw colours outside UiTheme: {hits:?}");
}

#[test]
fn a_raw_colour_counts_only_outside_the_theme_and_the_tests() {
    let theme = "impl Default for UiTheme {\n    a: Color::srgb_u8(1, 2, 3),\n}\n";
    assert!(raw_colours("ui.rs", theme).is_empty(), "the default theme");
    let widget = "fn f() {\n    Color::NONE;\n    Color::srgb(0.1, 0.2, 0.3);\n}\n";
    assert_eq!(
        raw_colours("ui.rs", widget),
        [3],
        "NONE passes, srgb does not"
    );
    let tested = "fn f() {}\n#[cfg(test)]\nfn t() { Color::WHITE; }\n";
    assert!(raw_colours("ui.rs", tested).is_empty(), "tests may");
    assert!(
        raw_colours("palette.rs", widget).is_empty(),
        "the adapter may"
    );
}

/// A literal screen size: `px(9)`, or a font size as a number. `px(width)` and
/// `px(at.x)` pass; world-space sizes are named consts in their modules.
fn literal_size(line: &str) -> bool {
    let digit_after = |open: &str| {
        line.match_indices(open).any(|(i, _)| {
            let rest = line[i + open.len()..].trim_start();
            rest.starts_with(|c: char| c.is_ascii_digit())
        })
    };
    let text_size = line.match_indices("text(").any(|(i, _)| {
        let args = line[i + 5..].split(')').next().unwrap_or_default();
        args.split(',')
            .skip(1)
            .any(|a| a.trim().starts_with(|c: char| c.is_ascii_digit()))
    });
    ["px(", "font_size(", "font_size:"]
        .iter()
        .any(|o| digit_after(o))
        || text_size
}

/// Literal sizes in a Boreas file's shipped code: every widget size is a
/// `UiMetrics` field, so one game's proportions are one resource.
fn literal_sizes(text: &str) -> Vec<usize> {
    shipped_hits(text, "UiMetrics", literal_size)
}

#[test]
fn boreas_widgets_take_sizes_from_the_metrics() {
    let src = root().join("crates/boreas/src");
    let mut hits = Vec::new();
    for path in files(&src) {
        let rel = path
            .strip_prefix(&src)
            .unwrap()
            .to_string_lossy()
            .into_owned();
        for line in literal_sizes(&fs::read_to_string(&path).unwrap()) {
            hits.push(format!("{rel}:{line}"));
        }
    }
    assert!(hits.is_empty(), "literal sizes outside UiMetrics: {hits:?}");
}

#[test]
fn a_literal_size_counts_only_outside_the_metrics_and_the_tests() {
    let metrics = "impl Default for UiMetrics {\n    a: 13.0,\n    b: px(4),\n}\n";
    assert!(literal_sizes(metrics).is_empty(), "the default metrics");
    let widget = "fn f() {\n    px(width);\n    px(at.x);\n    px(9);\n}\n";
    assert_eq!(literal_sizes(widget), [4], "names pass, a number does not");
    for size in [
        "TextFont::from_font_size(22.0)",
        "font_size: 18.0,",
        "text(label, 22.0, theme.foreground)",
        "UiRect::axes(px( 8), x)",
    ] {
        assert!(literal_size(size), "{size}");
    }
    for fine in [
        "text(label, m.button_text, c)",
        "TextFont::from_font_size(SIZE)",
        "percent(100)",
    ] {
        assert!(!literal_size(fine), "{fine}");
    }
    let tested = "fn f() {}\n#[cfg(test)]\nfn t() { px(36.0); }\n";
    assert!(literal_sizes(tested).is_empty(), "tests may");
}

/// A default block ends at its own closing brace: a literal after it still counts.
#[test]
fn a_literal_after_the_default_block_still_counts() {
    let metrics = "impl Default for UiMetrics {\n    a: px(4),\n}\n\nfn f() {\n    px(9);\n}\n";
    assert_eq!(
        literal_sizes(metrics),
        [6],
        "a size after UiMetrics' default"
    );
    let theme = "impl Default for UiTheme {\n    a: Color::srgb(0.0, 0.0, 0.0),\n}\nfn f() {\n    Color::WHITE;\n}\n";
    assert_eq!(
        raw_colours("ui/mod.rs", theme),
        [5],
        "a colour after UiTheme's default"
    );
}
