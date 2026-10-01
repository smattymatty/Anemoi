//! Bevy colours from an Aeolus `Palette` the game loads.
use aeolus::palette::{Palette, Rgb8};
use bevy::prelude::Color;

pub const fn color(rgb: Rgb8) -> Color {
    Color::srgb_u8(rgb.r, rgb.g, rgb.b)
}

pub const fn rgba(rgb: Rgb8, opacity: u8) -> Color {
    Color::srgba_u8(rgb.r, rgb.g, rgb.b, opacity)
}

/// The colour at `index`, or `None` past the palette's end.
pub fn at(palette: &Palette, index: usize) -> Option<Color> {
    palette.at(index).map(color)
}

pub fn alpha(palette: &Palette, index: usize, opacity: u8) -> Option<Color> {
    palette.at(index).map(|rgb| rgba(rgb, opacity))
}
