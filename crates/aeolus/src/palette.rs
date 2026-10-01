//! Indexed colour palettes parsed from GIMP `.gpl` text. Integers only.
use std::fmt;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Rgb8 {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Rgb8 {
    pub const fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }

    /// Squared RGB distance; at most 3 * 255², so it fits a `u32`.
    fn distance(self, other: Self) -> u32 {
        let d = |a: u8, b: u8| (a.abs_diff(b) as u32).pow(2);
        d(self.r, other.r) + d(self.g, other.g) + d(self.b, other.b)
    }
}

/// An ordered, non-empty list of colours. Order and duplicates are kept, so an
/// index means the same colour it does in GIMP.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Palette {
    name: Option<String>,
    colors: Vec<Rgb8>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ParseError {
    /// The first line is not `GIMP Palette`.
    MissingHeader,
    /// A 1-based line that is not `R G B [name]` with channels in 0..=255.
    BadRow(usize),
    NoColors,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingHeader => f.write_str("first line must be `GIMP Palette`"),
            Self::BadRow(line) => write!(f, "line {line}: expected `R G B [name]`, 0..=255"),
            Self::NoColors => f.write_str("palette has no colours"),
        }
    }
}

impl std::error::Error for ParseError {}

impl Palette {
    /// Parses GIMP palette text: the header, optional `Name:`/`Columns:` lines,
    /// `#` comments, blank lines, and `R G B [name]` rows.
    pub fn parse(text: &str) -> Result<Self, ParseError> {
        let mut lines = text.strip_prefix('\u{feff}').unwrap_or(text).lines();
        if lines.next().map(str::trim_end) != Some("GIMP Palette") {
            return Err(ParseError::MissingHeader);
        }
        let mut name = None;
        let mut colors = Vec::new();
        for (i, line) in lines.enumerate() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') || line.starts_with("Columns:") {
                continue;
            }
            if let Some(rest) = line.strip_prefix("Name:") {
                name = Some(rest.trim().to_owned());
                continue;
            }
            let mut channel = line.split_whitespace().map(str::parse::<u8>);
            match (channel.next(), channel.next(), channel.next()) {
                (Some(Ok(r)), Some(Ok(g)), Some(Ok(b))) => colors.push(Rgb8::new(r, g, b)),
                _ => return Err(ParseError::BadRow(i + 2)),
            }
        }
        if colors.is_empty() {
            return Err(ParseError::NoColors);
        }
        Ok(Self { name, colors })
    }

    pub fn name(&self) -> Option<&str> {
        self.name.as_deref()
    }

    pub fn colors(&self) -> &[Rgb8] {
        &self.colors
    }

    pub fn at(&self, index: usize) -> Option<Rgb8> {
        self.colors.get(index).copied()
    }

    pub fn contains(&self, rgb: Rgb8) -> bool {
        self.colors.contains(&rgb)
    }

    /// Index of the closest colour by squared RGB distance; ties go to the lowest index.
    pub fn nearest(&self, rgb: Rgb8) -> usize {
        let mut best = 0;
        for (i, color) in self.colors.iter().enumerate() {
            if color.distance(rgb) < self.colors[best].distance(rgb) {
                best = i;
            }
        }
        best
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST: &str = include_str!("../tests/data/test.gpl");

    fn test_palette() -> Palette {
        Palette::parse(TEST).unwrap()
    }

    #[test]
    fn parses_the_repo_test_palette_in_order_with_duplicates() {
        let p = test_palette();
        assert_eq!(p.name(), Some("Anemoi Test"));
        assert_eq!(p.colors().len(), 8);
        assert_eq!(p.at(0), Some(Rgb8::new(0, 0, 0)));
        assert_eq!(p.at(3), Some(Rgb8::new(200, 40, 40)), "a row with no name");
        assert_eq!(p.at(6), p.at(1), "duplicates keep their slot");
        assert_eq!(p.at(7), Some(Rgb8::new(10, 20, 30)));
        assert_eq!(p.at(8), None);
    }

    #[test]
    fn contains_only_exact_colours() {
        let p = test_palette();
        assert!(p.contains(Rgb8::new(40, 200, 40)));
        assert!(!p.contains(Rgb8::new(40, 200, 41)));
    }

    #[test]
    fn nearest_picks_the_closest_and_the_lowest_on_a_tie() {
        let p = test_palette();
        assert_eq!(p.nearest(Rgb8::new(255, 255, 255)), 1, "exact white");
        assert_eq!(p.nearest(Rgb8::new(190, 50, 30)), 3, "near red");
        assert_eq!(p.nearest(Rgb8::new(12, 18, 31)), 7, "near the last row");
        // Each channel alone pulls a pure query off black.
        assert_eq!(p.nearest(Rgb8::new(180, 0, 0)), 3);
        assert_eq!(p.nearest(Rgb8::new(0, 180, 0)), 2);
        assert_eq!(p.nearest(Rgb8::new(0, 0, 180)), 4);
        // Squared, not summed: 536 to the last row beats 576 to black.
        assert_eq!(p.nearest(Rgb8::new(0, 0, 24)), 7);
        // White sits at 1 and 6; the first copy wins.
        assert_eq!(p.nearest(Rgb8::new(250, 250, 250)), 1);
    }

    #[test]
    fn crlf_bom_and_extra_whitespace_parse() {
        let p = Palette::parse("\u{feff}GIMP Palette\r\n  1   2\t3  dust \r\n").unwrap();
        assert_eq!(p.colors(), &[Rgb8::new(1, 2, 3)]);
        assert_eq!(p.name(), None);
    }

    #[test]
    fn rejects_bad_input_with_its_line() {
        assert_eq!(
            Palette::parse("Name: x\n1 2 3"),
            Err(ParseError::MissingHeader)
        );
        assert_eq!(
            Palette::parse("GIMP Palette\n# c\n"),
            Err(ParseError::NoColors)
        );
        let row = |r: &str| Palette::parse(&format!("GIMP Palette\n0 0 0\n{r}\n"));
        assert_eq!(
            row("256 0 0"),
            Err(ParseError::BadRow(3)),
            "channel over 255"
        );
        assert_eq!(row("-1 0 0"), Err(ParseError::BadRow(3)));
        assert_eq!(row("1 2"), Err(ParseError::BadRow(3)), "too few channels");
        assert_eq!(row("red 0 0"), Err(ParseError::BadRow(3)));
    }
}
