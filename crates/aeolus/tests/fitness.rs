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
        "crates/boreas/src/ui.rs",
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
