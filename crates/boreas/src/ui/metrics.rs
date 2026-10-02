//! Every widget size in one resource, and the system that applies it.
use bevy::prelude::*;

/// Every size the widgets use, apart from colours: a classic context menu's
/// proportions at a 16 px cell. A game may insert its own; [`fit`] applies it.
#[derive(Resource, Clone, Copy, Debug, PartialEq)]
pub struct UiMetrics {
    /// Every widget's edge and every panel's frame.
    pub border: f32,
    pub row_height: f32,
    /// Between a refused row's label and its reason.
    pub row_spacing: f32,
    pub row_pad: Vec2,
    pub row_text: f32,
    pub menu_row_gap: f32,
    pub menu_inset: f32,
    pub menu_min_width: f32,
    /// Between the Tile Menu and its cell.
    pub menu_cell_gap: f32,
    pub button_height: f32,
    pub button_pad: Vec2,
    pub button_text: f32,
    /// From the screen's top-right corner.
    pub corner_offset: f32,
    pub corner_pad: Vec2,
    pub corner_text: f32,
    pub panel_gap: f32,
    pub panel_pad: f32,
    /// The widest a panel gets, in percent of the screen.
    pub panel_max_width: f32,
}

impl Default for UiMetrics {
    fn default() -> Self {
        Self {
            border: 1.0,
            row_height: 20.0,
            row_spacing: 8.0,
            row_pad: Vec2::new(8.0, 1.0),
            row_text: 13.0,
            menu_row_gap: 4.0,
            menu_inset: 2.0,
            menu_min_width: 72.0,
            menu_cell_gap: 4.0,
            button_height: 48.0,
            button_pad: Vec2::new(16.0, 8.0),
            button_text: 22.0,
            corner_offset: 20.0,
            corner_pad: Vec2::new(18.0, 11.0),
            corner_text: 18.0,
            panel_gap: 12.0,
            panel_pad: 20.0,
            panel_max_width: 92.0,
        }
    }
}

/// A widget's kind: which [`UiMetrics`] sizes it takes, and (a corner) its resting edge.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Fit {
    Panel,
    Menu,
    Button,
    Row,
    Corner,
}

/// Sizes every [`Fit`] widget and its text from [`UiMetrics`]: on spawn, and all
/// of them again when a game changes the metrics.
pub(super) fn fit(
    metrics: Res<UiMetrics>,
    mut widgets: Query<(Ref<Fit>, &mut Node, Option<&Children>)>,
    mut fonts: Query<&mut TextFont>,
) {
    let m = *metrics;
    let axes = |pad: Vec2| UiRect::axes(px(pad.x), px(pad.y));
    for (shape, mut node, children) in &mut widgets {
        if !(shape.is_added() || metrics.is_changed()) {
            continue;
        }
        node.border = UiRect::all(px(m.border));
        let text = match *shape {
            Fit::Panel => {
                node.max_width = percent(m.panel_max_width);
                node.row_gap = px(m.panel_gap);
                node.padding = UiRect::all(px(m.panel_pad));
                None
            }
            Fit::Menu => {
                node.row_gap = px(m.menu_row_gap);
                node.padding = UiRect::all(px(m.menu_inset));
                node.min_width = px(m.menu_min_width);
                None
            }
            Fit::Button => {
                node.height = px(m.button_height);
                node.padding = axes(m.button_pad);
                Some(m.button_text)
            }
            Fit::Row => {
                node.height = px(m.row_height);
                node.column_gap = px(m.row_spacing);
                node.padding = axes(m.row_pad);
                Some(m.row_text)
            }
            Fit::Corner => {
                (node.top, node.right) = (px(m.corner_offset), px(m.corner_offset));
                node.padding = axes(m.corner_pad);
                Some(m.corner_text)
            }
        };
        let Some(size) = text else { continue };
        for child in children.into_iter().flat_map(|c| c.iter()) {
            if let Ok(mut font) = fonts.get_mut(child) {
                font.font_size = size.into();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests::menu;
    use super::super::*;

    /// A widget as sized: its node and its texts' font sizes.
    fn sized(app: &App, widget: Entity) -> (Node, Vec<f32>) {
        let w = app.world();
        let fonts = w.get::<Children>(widget).into_iter().flat_map(|c| c.iter());
        let fonts = fonts
            .filter_map(|c| w.get::<TextFont>(c))
            .map(|f| match f.font_size {
                bevy::text::FontSize::Px(px) => px,
                other => panic!("not px: {other:?}"),
            });
        (w.get::<Node>(widget).unwrap().clone(), fonts.collect())
    }

    /// Every widget takes its sizes from `UiMetrics`: the defaults, a game's own,
    /// and the game's again when it changes them later.
    #[test]
    fn every_widget_takes_its_sizes_from_the_metrics() {
        let (mut app, _, [b, _]) = menu();
        let theme = UiTheme::default();
        let w = app.world_mut();
        let p = w.spawn(panel(&theme, 300.0)).id();
        let m = w.spawn(menu_panel(0, &theme)).id();
        let r = w.spawn(refused_row("Open", "shut", 0, &theme)).id();
        let c = w.spawn(corner_button("MENU", &theme)).id();
        let check = |app: &App, s: UiMetrics| {
            let all = |v| UiRect::all(px(v));
            let axes = |v: Vec2| UiRect::axes(px(v.x), px(v.y));
            let (n, t) = sized(app, p);
            let want = (all(s.border), all(s.panel_pad), px(s.panel_gap));
            assert_eq!((n.border, n.padding, n.row_gap), want, "panel");
            assert_eq!((n.max_width, t.len()), (percent(s.panel_max_width), 0));
            let (n, _) = sized(app, m);
            let want = (all(s.border), all(s.menu_inset), px(s.menu_row_gap));
            assert_eq!((n.border, n.padding, n.row_gap), want, "menu");
            assert_eq!(n.min_width, px(s.menu_min_width));
            let (n, t) = sized(app, b);
            let want = (all(s.border), axes(s.button_pad), px(s.button_height));
            assert_eq!(
                (n.border, n.padding, n.height, t),
                (want.0, want.1, want.2, vec![s.button_text]),
                "button"
            );
            let (n, t) = sized(app, r);
            let want = (
                all(s.border),
                axes(s.row_pad),
                px(s.row_height),
                px(s.row_spacing),
            );
            assert_eq!((n.border, n.padding, n.height, n.column_gap), want, "row");
            assert_eq!(t, [s.row_text; 2], "both texts of a refused row");
            let (n, t) = sized(app, c);
            let at = px(s.corner_offset);
            assert_eq!(
                (n.border, n.padding, n.top, n.right),
                (all(s.border), axes(s.corner_pad), at, at),
                "corner"
            );
            assert_eq!(t, [s.corner_text]);
        };
        app.update();
        check(&app, UiMetrics::default());
        let mut own = UiMetrics::default();
        for (i, f) in [
            &mut own.border,
            &mut own.row_height,
            &mut own.row_spacing,
            &mut own.row_text,
            &mut own.menu_row_gap,
            &mut own.menu_inset,
            &mut own.menu_min_width,
            &mut own.button_height,
            &mut own.button_text,
            &mut own.corner_offset,
            &mut own.corner_text,
            &mut own.panel_gap,
            &mut own.panel_pad,
            &mut own.panel_max_width,
        ]
        .into_iter()
        .enumerate()
        {
            *f = 30.0 + i as f32;
        }
        (own.row_pad, own.button_pad, own.corner_pad) = (
            Vec2::new(5.0, 6.0),
            Vec2::new(7.0, 9.0),
            Vec2::new(10.0, 3.0),
        );
        app.insert_resource(own);
        app.update();
        check(&app, own);
    }
}
