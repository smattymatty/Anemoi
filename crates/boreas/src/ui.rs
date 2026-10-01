//! Reusable menu primitives: theme, focus, keyboard navigation, and pointer activation.
use crate::palette;
use aeolus::palette::Palette;
use bevy::audio::Volume;
use bevy::input::mouse::MouseMotion;
use bevy::prelude::*;

/// Resolved menu colours. `Default` is a neutral grey; games build theirs with
/// [`UiTheme::from_palette`].
#[derive(Resource, Clone, Copy, Debug, PartialEq)]
pub struct UiTheme {
    pub panel: Color,
    pub button: Color,
    pub active: Color,
    pub pressed: Color,
    pub foreground: Color,
    pub muted: Color,
    pub accent: Color,
    pub veil: Color,
}

/// A theme as palette indices; the veil also takes an opacity.
#[derive(Clone, Copy, Debug)]
pub struct ThemeIndices {
    pub panel: u8,
    pub button: u8,
    pub active: u8,
    pub pressed: u8,
    pub foreground: u8,
    pub muted: u8,
    pub accent: u8,
    pub veil: u8,
    pub veil_alpha: u8,
}

/// A theme index past the palette's end.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OutOfPalette(pub u8);

impl UiTheme {
    /// Resolves every index once, so builders and styling never touch the palette.
    pub fn from_palette(source: &Palette, ix: ThemeIndices) -> Result<Self, OutOfPalette> {
        let at = |i: u8| palette::at(source, i.into()).ok_or(OutOfPalette(i));
        Ok(Self {
            panel: at(ix.panel)?,
            button: at(ix.button)?,
            active: at(ix.active)?,
            pressed: at(ix.pressed)?,
            foreground: at(ix.foreground)?,
            muted: at(ix.muted)?,
            accent: at(ix.accent)?,
            veil: palette::alpha(source, ix.veil.into(), ix.veil_alpha)
                .ok_or(OutOfPalette(ix.veil))?,
        })
    }
}

impl Default for UiTheme {
    fn default() -> Self {
        Self {
            panel: Color::srgb_u8(24, 24, 28),
            button: Color::srgb_u8(52, 52, 60),
            active: Color::srgb_u8(84, 84, 96),
            pressed: Color::srgb_u8(36, 36, 42),
            foreground: Color::srgb_u8(232, 232, 232),
            muted: Color::srgb_u8(140, 140, 148),
            accent: Color::srgb_u8(220, 184, 88),
            veil: Color::srgba_u8(0, 0, 0, 235),
        }
    }
}

/// Which device leads menus. Keys take over the moment one is pressed; the pointer
/// only by moving or clicking, so an idle cursor over a button never steals focus.
#[derive(Resource, Default, Clone, Copy, Debug, PartialEq, Eq)]
pub enum UiLead {
    #[default]
    Keys,
    Pointer,
}

const NAV_KEYS: [KeyCode; 7] = [
    KeyCode::ArrowUp,
    KeyCode::ArrowDown,
    KeyCode::KeyW,
    KeyCode::KeyS,
    KeyCode::Tab,
    KeyCode::Enter,
    KeyCode::Space,
];

fn track_lead(
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    mut motion: MessageReader<MouseMotion>,
    mut cursor: MessageReader<CursorMoved>,
    mut lead: ResMut<UiLead>,
) {
    let moved = motion.read().any(|m| m.delta != Vec2::ZERO) | (cursor.read().count() > 0);
    if moved || mouse.get_just_pressed().next().is_some() {
        lead.set_if_neq(UiLead::Pointer);
    }
    if NAV_KEYS.iter().any(|&k| keys.just_pressed(k)) {
        lead.set_if_neq(UiLead::Keys);
    }
}

/// Counted in headless runs so input feedback stays inspectable.
#[derive(Resource, Default)]
pub struct UiFeedbackStats {
    pub focus: u32,
    pub activate: u32,
}

/// Optional game-supplied sounds. The UI plugin owns when to play them.
#[derive(Resource)]
pub struct UiSounds {
    pub focus: Handle<AudioSource>,
    pub activate: Handle<AudioSource>,
    pub volume: f32,
}

#[derive(Message)]
pub struct UiFocused {
    pub entity: Entity,
}

/// Attach to a panel whose immediate children are focusable buttons.
#[derive(Component)]
pub struct MenuSelection {
    pub selected: usize,
    pub count: usize,
}

#[derive(Component)]
pub struct Focusable(pub usize);

#[derive(Component)]
pub struct StyledButton;

#[derive(Message)]
pub struct UiActivated {
    pub entity: Entity,
}

pub fn overlay(theme: &UiTheme) -> impl Bundle {
    (
        Node {
            position_type: PositionType::Absolute,
            width: percent(100),
            height: percent(100),
            align_items: AlignItems::Center,
            justify_content: JustifyContent::Center,
            ..default()
        },
        BackgroundColor(theme.veil),
        Visibility::default(),
    )
}

pub fn panel(theme: &UiTheme, width: f32) -> impl Bundle {
    (
        Node {
            width: px(width),
            max_width: percent(92),
            flex_direction: FlexDirection::Column,
            row_gap: px(12),
            padding: UiRect::all(px(20)),
            border: UiRect::all(px(3)),
            ..default()
        },
        BackgroundColor(theme.panel),
        BorderColor::all(theme.accent),
        Visibility::default(),
    )
}

pub fn text(label: impl Into<String>, size: f32, color: Color) -> impl Bundle {
    (
        Text::new(label),
        TextFont::from_font_size(size),
        TextColor(color),
    )
}

pub fn button(label: impl Into<String>, index: usize, theme: &UiTheme) -> impl Bundle {
    (
        Button,
        StyledButton,
        Focusable(index),
        Node {
            width: percent(100),
            height: px(48),
            align_items: AlignItems::Center,
            padding: UiRect::axes(px(16), px(8)),
            border: UiRect::all(px(2)),
            ..default()
        },
        BackgroundColor(theme.button),
        BorderColor::all(theme.muted),
        Visibility::default(),
        children![text(label, 22.0, theme.foreground)],
    )
}

pub fn corner_button(label: impl Into<String>, theme: &UiTheme) -> impl Bundle {
    (
        Button,
        StyledButton,
        Focusable(0),
        Node {
            position_type: PositionType::Absolute,
            top: px(20),
            right: px(20),
            padding: UiRect::axes(px(18), px(11)),
            border: UiRect::all(px(2)),
            ..default()
        },
        BackgroundColor(theme.button),
        BorderColor::all(theme.muted),
        Visibility::default(),
        children![text(label, 18.0, theme.foreground)],
    )
}

/// Sends the same activation message for a click or Enter/Space on the focused button.
/// A click always activates. Hover selects only while the pointer leads, and
/// then every frame, so a moving mouse takes focus back from the keys at once.
pub fn navigate_and_activate(
    keys: Res<ButtonInput<KeyCode>>,
    lead: Res<UiLead>,
    mut menus: Query<(Entity, &mut MenuSelection)>,
    buttons: Query<(Entity, Ref<Interaction>, &Focusable, Option<&ChildOf>), With<StyledButton>>,
    mut activated: MessageWriter<UiActivated>,
    mut focused: MessageWriter<UiFocused>,
) {
    for (entity, interaction, focusable, parent) in &buttons {
        if interaction.is_changed() && *interaction == Interaction::Pressed {
            activated.write(UiActivated { entity });
        }
        if *lead != UiLead::Pointer || *interaction != Interaction::Hovered {
            continue;
        }
        let menu = parent.and_then(|p| menus.get_mut(p.parent()).ok());
        match menu {
            Some((_, mut menu)) if menu.selected != focusable.0 => {
                menu.selected = focusable.0;
                focused.write(UiFocused { entity });
            }
            None if interaction.is_changed() || lead.is_changed() => {
                focused.write(UiFocused { entity });
            }
            _ => {}
        }
    }
    for (menu_entity, mut menu) in &mut menus {
        if menu.count == 0 {
            continue;
        }
        let before = menu.selected;
        if keys.just_pressed(KeyCode::ArrowUp) || keys.just_pressed(KeyCode::KeyW) {
            menu.selected = (menu.selected + menu.count - 1) % menu.count;
        }
        if keys.just_pressed(KeyCode::ArrowDown)
            || keys.just_pressed(KeyCode::KeyS)
            || keys.just_pressed(KeyCode::Tab)
        {
            menu.selected = (menu.selected + 1) % menu.count;
        }
        if menu.selected != before {
            if let Some((entity, _, _, _)) = buttons.iter().find(|(_, _, f, parent)| {
                f.0 == menu.selected && parent.is_some_and(|p| p.parent() == menu_entity)
            }) {
                focused.write(UiFocused { entity });
            }
        }
        if keys.just_pressed(KeyCode::Enter) || keys.just_pressed(KeyCode::Space) {
            if let Some((entity, _, _, _)) = buttons.iter().find(|(_, _, f, parent)| {
                f.0 == menu.selected && parent.is_some_and(|p| p.parent() == menu_entity)
            }) {
                activated.write(UiActivated { entity });
            }
        }
    }
}

fn style_buttons(
    theme: Res<UiTheme>,
    lead: Res<UiLead>,
    menus: Query<&MenuSelection>,
    mut buttons: Query<
        (
            &Focusable,
            &Interaction,
            Option<&ChildOf>,
            &mut BackgroundColor,
            &mut BorderColor,
        ),
        With<StyledButton>,
    >,
) {
    for (index, interaction, parent, mut background, mut border) in &mut buttons {
        let focused = parent
            .and_then(|p| menus.get(p.parent()).ok())
            .is_some_and(|menu| menu.selected == index.0);
        let hovered = *lead == UiLead::Pointer && *interaction == Interaction::Hovered;
        let active = focused || hovered;
        let fill = if *interaction == Interaction::Pressed {
            theme.pressed
        } else if active {
            theme.active
        } else {
            theme.button
        };
        *background = BackgroundColor(fill);
        *border = BorderColor::all(if active || *interaction == Interaction::Pressed {
            theme.accent
        } else {
            theme.muted
        });
    }
}

fn play_feedback(
    mut commands: Commands,
    sounds: Option<Res<UiSounds>>,
    mut stats: ResMut<UiFeedbackStats>,
    mut focus: MessageReader<UiFocused>,
    mut activate: MessageReader<UiActivated>,
) {
    let focused = focus.read().count();
    let activated = activate.read().count();
    let Some(sounds) = sounds else { return };
    stats.focus += focused as u32;
    stats.activate += activated as u32;
    let settings = PlaybackSettings::DESPAWN.with_volume(Volume::Linear(sounds.volume));
    for _ in 0..focused {
        commands.spawn((AudioPlayer(sounds.focus.clone()), settings.clone()));
    }
    for _ in 0..activated {
        commands.spawn((AudioPlayer(sounds.activate.clone()), settings.clone()));
    }
}

pub struct UiPlugin;

impl Plugin for UiPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<UiTheme>()
            .init_resource::<UiFeedbackStats>()
            .init_resource::<UiLead>()
            .add_message::<MouseMotion>()
            .add_message::<CursorMoved>()
            .add_message::<UiActivated>()
            .add_message::<UiFocused>()
            .add_systems(
                Update,
                (
                    track_lead,
                    navigate_and_activate,
                    style_buttons,
                    play_feedback,
                )
                    .chain(),
            );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A two-button menu; the returned buttons are focus 0 and 1.
    fn menu() -> (App, Entity, [Entity; 2]) {
        let mut app = App::new();
        app.insert_resource(ButtonInput::<KeyCode>::default())
            .insert_resource(ButtonInput::<MouseButton>::default())
            .insert_resource(UiSounds {
                focus: Handle::default(),
                activate: Handle::default(),
                volume: 0.55,
            })
            .add_plugins(UiPlugin);
        let panel = app
            .world_mut()
            .spawn(MenuSelection {
                selected: 0,
                count: 2,
            })
            .id();
        let button = |app: &mut App, i| {
            let id = app
                .world_mut()
                .spawn((
                    StyledButton,
                    Focusable(i),
                    Interaction::None,
                    BackgroundColor(UiTheme::default().button),
                    BorderColor::all(UiTheme::default().muted),
                ))
                .id();
            app.world_mut().entity_mut(panel).add_child(id);
            id
        };
        let buttons = [button(&mut app, 0), button(&mut app, 1)];
        app.update();
        (app, panel, buttons)
    }

    fn tap(app: &mut App, key: KeyCode) {
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(key);
        app.update();
        let mut keys = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
        keys.release(key);
        keys.clear();
    }

    fn nudge_mouse(app: &mut App) {
        app.world_mut()
            .write_message(MouseMotion { delta: Vec2::X });
        app.update();
    }

    fn selected(app: &App, panel: Entity) -> usize {
        app.world().get::<MenuSelection>(panel).unwrap().selected
    }

    fn fill(app: &App, button: Entity) -> Color {
        app.world().get::<BackgroundColor>(button).unwrap().0
    }

    const INDICES: ThemeIndices = ThemeIndices {
        panel: 0,
        button: 1,
        active: 2,
        pressed: 3,
        foreground: 4,
        muted: 5,
        accent: 7,
        veil: 6,
        veil_alpha: 99,
    };

    fn test_palette() -> Palette {
        Palette::parse(include_str!("../../aeolus/tests/data/test.gpl")).unwrap()
    }

    #[test]
    fn from_palette_resolves_each_index_to_its_own_colour() {
        let theme = UiTheme::from_palette(&test_palette(), INDICES).unwrap();
        let rgb = Color::srgb_u8;
        assert_eq!(theme.panel, rgb(0, 0, 0));
        assert_eq!(theme.button, rgb(255, 255, 255));
        assert_eq!(theme.active, rgb(40, 200, 40));
        assert_eq!(theme.pressed, rgb(200, 40, 40));
        assert_eq!(theme.foreground, rgb(40, 40, 200));
        assert_eq!(theme.muted, rgb(128, 128, 128));
        assert_eq!(theme.accent, rgb(10, 20, 30));
        assert_eq!(theme.veil, Color::srgba_u8(255, 255, 255, 99));
    }

    #[test]
    fn from_palette_names_the_first_index_past_the_end() {
        let ix = ThemeIndices {
            muted: 8,
            veil: 200,
            ..INDICES
        };
        assert_eq!(
            UiTheme::from_palette(&test_palette(), ix),
            Err(OutOfPalette(8))
        );
        let ix = ThemeIndices { veil: 8, ..INDICES };
        assert_eq!(
            UiTheme::from_palette(&test_palette(), ix),
            Err(OutOfPalette(8))
        );
    }

    /// The classic bug: a menu opens under a resting cursor. The keys keep the
    /// menu, one highlight only, until the mouse actually moves.
    #[test]
    fn an_idle_cursor_never_steals_focus_from_the_keys() {
        let (mut app, panel, [first, second]) = menu();
        app.world_mut()
            .entity_mut(second)
            .insert(Interaction::Hovered);
        app.update();
        assert_eq!(selected(&app, panel), 0, "resting hover selects nothing");
        assert_eq!(
            fill(&app, second),
            UiTheme::default().button,
            "and isn't lit"
        );

        tap(&mut app, KeyCode::KeyS);
        assert_eq!(selected(&app, panel), 1);
        tap(&mut app, KeyCode::KeyW);
        assert_eq!(selected(&app, panel), 0, "W wins over the idle hover");
        assert_eq!(fill(&app, first), UiTheme::default().active);
        assert_eq!(
            fill(&app, second),
            UiTheme::default().button,
            "one highlight"
        );

        nudge_mouse(&mut app);
        assert_eq!(selected(&app, panel), 1, "a moving mouse takes over");
        assert_eq!(fill(&app, first), UiTheme::default().button);

        tap(&mut app, KeyCode::ArrowDown);
        assert_eq!(selected(&app, panel), 0, "a key takes it straight back");
    }

    #[test]
    fn pointer_hover_and_press_share_focus_and_activation_feedback() {
        let (mut app, panel, [_, button]) = menu();
        app.world_mut()
            .entity_mut(button)
            .insert(Interaction::Hovered);
        nudge_mouse(&mut app);
        assert_eq!(selected(&app, panel), 1);
        let stats = app.world().resource::<UiFeedbackStats>();
        assert_eq!((stats.focus, stats.activate), (1, 0));
        assert_eq!(fill(&app, button), UiTheme::default().active);
        app.world_mut()
            .entity_mut(button)
            .insert(Interaction::Pressed);
        app.update();
        let stats = app.world().resource::<UiFeedbackStats>();
        assert_eq!((stats.focus, stats.activate), (1, 1));
        assert_eq!(fill(&app, button), UiTheme::default().pressed);
    }
}
