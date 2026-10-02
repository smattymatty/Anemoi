//! The one look every focusable widget wears, and the roles it was last drawn in.
use super::metrics::Fit;
use super::{Focusable, MenuSelection, PointerReach, UiLead, UiTheme};
use bevy::prelude::*;

/// The one look of every focusable widget (a classic context menu): bare until lit,
/// then filled `active`, edged and lettered `accent`; pressed draws as lit. A refused
/// widget keeps muted letters; a corner button keeps a 1 px `button` edge unlit.
#[derive(Component, Clone, Copy, Debug, Default)]
#[require(Drawn, BackgroundColor, BorderColor)]
pub struct Look {
    pub refused: bool,
}

/// A [`UiTheme`] role. The `ui` dump names roles, never colours: two may share one.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Role {
    #[default]
    None,
    Button,
    Active,
    Accent,
    Foreground,
    Muted,
}

impl Role {
    pub fn color(self, theme: &UiTheme) -> Color {
        match self {
            Role::None => Color::NONE,
            Role::Button => theme.button,
            Role::Active => theme.active,
            Role::Accent => theme.accent,
            Role::Foreground => theme.foreground,
            Role::Muted => theme.muted,
        }
    }

    fn name(self) -> &'static str {
        match self {
            Role::None => "none",
            Role::Button => "button",
            Role::Active => "active",
            Role::Accent => "accent",
            Role::Foreground => "foreground",
            Role::Muted => "muted",
        }
    }
}

/// The roles [`style`] last drew a [`Look`] in; the `ui` dump reads it.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Drawn {
    pub lit: bool,
    pub fill: Role,
    pub edge: Role,
    pub ink: Role,
}

/// Draws every [`Look`]: lit when focused in its menu, or hovered or pressed while
/// the pointer leads and reaches it.
pub(super) fn style(
    theme: Res<UiTheme>,
    lead: Res<UiLead>,
    menus: Query<&MenuSelection>,
    reach: PointerReach,
    mut widgets: Query<(
        (&Focusable, &Interaction, &Look, Option<&Fit>),
        Option<&ChildOf>,
        Option<&Children>,
        &mut Drawn,
        &mut BackgroundColor,
        &mut BorderColor,
    )>,
    mut letters: Query<&mut TextColor>,
) {
    for ((index, interaction, look, fit), parent, children, mut drawn, mut fill, mut edge) in
        &mut widgets
    {
        let focused = parent
            .and_then(|p| menus.get(p.parent()).ok())
            .is_some_and(|menu| menu.selected == index.0);
        let pointed = *lead == UiLead::Pointer && *interaction != Interaction::None;
        let lit = focused || (pointed && reach.reaches(parent));
        let resting = if fit == Some(&Fit::Corner) {
            Role::Button
        } else {
            Role::None
        };
        let ink = match (look.refused, lit) {
            (true, _) => Role::Muted,
            (false, true) => Role::Accent,
            (false, false) => Role::Foreground,
        };
        *drawn = Drawn {
            lit,
            fill: if lit { Role::Active } else { Role::None },
            edge: if lit { Role::Accent } else { resting },
            ink,
        };
        *fill = BackgroundColor(drawn.fill.color(&theme));
        *edge = BorderColor::all(drawn.edge.color(&theme));
        for child in children.into_iter().flat_map(|c| c.iter()) {
            if let Ok(mut c) = letters.get_mut(child) {
                c.0 = drawn.ink.color(&theme);
            }
        }
    }
}

/// A menu's lit widget, by the roles it was drawn in, for the `ui` dump.
pub(super) fn lit(w: &World, menu: Entity) -> Option<String> {
    let (index, d) = w.get::<Children>(menu)?.iter().find_map(|row| {
        let d = w.get::<Drawn>(row).filter(|d| d.lit)?;
        Some((w.get::<Focusable>(row)?.0, d))
    })?;
    let (fill, edge, ink) = (d.fill.name(), d.edge.name(), d.ink.name());
    Some(format!("lit={index} fill={fill} edge={edge} ink={ink}"))
}

#[cfg(test)]
mod tests {
    use super::super::tests::{fill, menu, nudge_mouse, tap, ui_line};
    use super::super::*;

    /// A widget as drawn: fill, edge, first letters.
    fn drawn(app: &App, widget: Entity) -> (Color, Color, Color) {
        let w = app.world();
        let ink = w
            .get::<TextColor>(w.get::<Children>(widget).unwrap()[0])
            .unwrap()
            .0;
        (
            w.get::<BackgroundColor>(widget).unwrap().0,
            w.get::<BorderColor>(widget).unwrap().top,
            ink,
        )
    }

    /// One look: a full-size `button` and a compact row in one menu draw the same
    /// roles, lit and unlit.
    #[test]
    fn a_button_and_a_row_share_one_look() {
        let (mut app, panel, [b, _]) = menu();
        let theme = UiTheme::default();
        let r = app.world_mut().spawn(row("Look", 1, &theme)).id();
        app.world_mut().entity_mut(panel).add_child(r);
        app.update();
        let lit = (theme.active, theme.accent, theme.accent);
        let bare = (Color::NONE, Color::NONE, theme.foreground);
        assert_eq!((drawn(&app, b), drawn(&app, r)), (lit, bare));
        tap(&mut app, KeyCode::KeyS);
        assert_eq!((drawn(&app, b), drawn(&app, r)), (bare, lit));
    }

    /// A navigable widget is always drawn: a bare `StyledButton` brings its `Look`,
    /// so a focused one never navigates unlit.
    #[test]
    fn a_styled_button_is_always_drawn() {
        let (mut app, panel, _) = menu();
        let bare = (StyledButton, Focusable(1));
        let bare = app.world_mut().spawn(bare).id();
        app.world_mut().entity_mut(panel).add_child(bare);
        tap(&mut app, KeyCode::KeyS);
        assert_eq!(fill(&app, bare), UiTheme::default().active);
    }

    /// An always-visible corner button keeps a subtle `button` edge at rest, so it
    /// stays findable; lit, it is any other widget.
    #[test]
    fn a_corner_button_rests_edged_and_lights_as_one_look() {
        let (mut app, _, _) = menu();
        let theme = UiTheme::default();
        let lone = app
            .world_mut()
            .spawn(MenuSelection {
                selected: 1,
                count: 1,
            })
            .id();
        let corner = app.world_mut().spawn(corner_button("MENU", &theme)).id();
        app.world_mut().entity_mut(lone).add_child(corner);
        app.update();
        let rest = (Color::NONE, theme.button, theme.foreground);
        assert_eq!(drawn(&app, corner), rest);
        app.world_mut()
            .entity_mut(corner)
            .insert(Interaction::Hovered);
        nudge_mouse(&mut app);
        assert_eq!(
            drawn(&app, corner),
            (theme.active, theme.accent, theme.accent)
        );
    }

    /// Both panels take the Tile Menu's subtle frame: 1 px, `button`, on `panel`.
    #[test]
    fn a_panel_has_a_subtle_frame() {
        let (mut app, _, _) = menu();
        let theme = UiTheme::default();
        let p = app.world_mut().spawn(panel(&theme, 300.0)).id();
        let m = app.world_mut().spawn(menu_panel(0, &theme)).id();
        app.update();
        let w = app.world();
        for p in [p, m] {
            assert_eq!(w.get::<Node>(p).unwrap().border, UiRect::all(px(1)));
            assert_eq!(w.get::<BorderColor>(p).unwrap().top, theme.button);
            assert_eq!(w.get::<BackgroundColor>(p).unwrap().0, theme.panel);
        }
    }

    /// The dump names the roles the lit widget was drawn in, never maps colours back:
    /// a theme whose accent is its active colour still reads `fill=active edge=accent`.
    #[test]
    fn the_ui_dump_names_the_lit_roles_not_their_colours() {
        let (mut app, panel, _) = menu();
        let mut twins = UiTheme::default();
        twins.accent = twins.active;
        app.insert_resource(twins);
        tap(&mut app, KeyCode::KeyS);
        let lit = " lit=1 fill=active edge=accent ink=accent";
        assert!(ui_line(&app).ends_with(lit), "{}", ui_line(&app));
        app.world_mut().entity_mut(panel).remove::<TakesInput>();
        app.update();
        assert!(ui_line(&app).ends_with(" lit=none"), "{}", ui_line(&app));
    }

    /// A classic context menu: rows are bare until focused; the focused one is filled,
    /// edged and lettered in the accent; a refused row keeps muted letters.
    #[test]
    fn a_compact_row_lights_only_when_focused() {
        let mut app = App::new();
        app.insert_resource(ButtonInput::<KeyCode>::default())
            .insert_resource(ButtonInput::<MouseButton>::default())
            .add_plugins(UiPlugin);
        let theme = UiTheme::default();
        let panel = app
            .world_mut()
            .spawn((
                MenuSelection {
                    selected: 0,
                    count: 3,
                },
                TakesInput,
            ))
            .id();
        let rows = [
            app.world_mut().spawn(row("Move", 0, &theme)).id(),
            app.world_mut().spawn(row("Look", 1, &theme)).id(),
            app.world_mut()
                .spawn(refused_row("Open", "shut", 2, &theme))
                .id(),
        ];
        for r in rows {
            app.world_mut().entity_mut(r).insert(Interaction::None);
            app.world_mut().entity_mut(panel).add_child(r);
        }
        app.update();
        let look = drawn;
        assert_eq!(
            look(&app, rows[0]),
            (theme.active, theme.accent, theme.accent)
        );
        assert_eq!(
            look(&app, rows[1]),
            (Color::NONE, Color::NONE, theme.foreground)
        );
        app.world_mut()
            .get_mut::<MenuSelection>(panel)
            .unwrap()
            .selected = 2;
        app.update();
        assert_eq!(
            look(&app, rows[0]),
            (Color::NONE, Color::NONE, theme.foreground)
        );
        assert_eq!(
            look(&app, rows[2]),
            (theme.active, theme.accent, theme.muted)
        );
    }
}
