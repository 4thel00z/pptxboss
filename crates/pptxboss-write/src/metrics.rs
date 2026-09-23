//! Text measurement from embedded font metrics: which open face stands in
//! for a font name, how wide a string is, how tall a line is, and how many
//! lines a paragraph takes at a given width.

use crate::metrics_data as data;
use crate::{Paragraph, EMU_PER_POINT};

/// An open font whose advance widths match a common presentation font.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Face {
    /// Calibri.
    Carlito,
    /// Cambria.
    Caladea,
    /// Arial, Helvetica and most other sans-serif fonts.
    LiberationSans,
    /// Times New Roman and most other serif fonts.
    LiberationSerif,
    /// Courier New and other monospaced fonts.
    LiberationMono,
    /// Georgia.
    Gelasio,
}

impl Face {
    /// The face whose metrics stand in for `font`; unknown sans-serif names
    /// take Liberation Sans, whose glyphs are wider than Calibri's, so an
    /// estimate errs toward more lines rather than fewer.
    pub(crate) fn for_font(font: &str) -> Face {
        let name = font.to_ascii_lowercase();
        let has = |needle: &str| name.contains(needle);
        if has("mono") || has("menlo") || has("consolas") || has("courier") || has("code") {
            return Face::LiberationMono;
        }
        if has("georgia") || has("gelasio") {
            return Face::Gelasio;
        }
        if has("cambria") || has("caladea") {
            return Face::Caladea;
        }
        if has("calibri") || has("carlito") {
            return Face::Carlito;
        }
        let serif = has("times")
            || has("garamond")
            || has("palatino")
            || has("baskerville")
            || has("book")
            || has("charter")
            || (has("serif") && !has("sans"));
        match serif {
            true => Face::LiberationSerif,
            false => Face::LiberationSans,
        }
    }

    fn table(self, bold: bool) -> &'static [u16; 224] {
        match (self, bold) {
            (Face::Carlito, false) => &data::CARLITO_REGULAR,
            (Face::Carlito, true) => &data::CARLITO_BOLD,
            (Face::Caladea, false) => &data::CALADEA_REGULAR,
            (Face::Caladea, true) => &data::CALADEA_BOLD,
            (Face::LiberationSans, false) => &data::LIBERATION_SANS_REGULAR,
            (Face::LiberationSans, true) => &data::LIBERATION_SANS_BOLD,
            (Face::LiberationSerif, false) => &data::LIBERATION_SERIF_REGULAR,
            (Face::LiberationSerif, true) => &data::LIBERATION_SERIF_BOLD,
            (Face::LiberationMono, false) => &data::LIBERATION_MONO_REGULAR,
            (Face::LiberationMono, true) => &data::LIBERATION_MONO_BOLD,
            (Face::Gelasio, false) => &data::GELASIO_REGULAR,
            (Face::Gelasio, true) => &data::GELASIO_BOLD,
        }
    }

    /// Ascent plus descent plus line gap, in thousandths of an em.
    fn line_height(self) -> u32 {
        match self {
            Face::Carlito => data::CARLITO_LINE_HEIGHT,
            Face::Caladea => data::CALADEA_LINE_HEIGHT,
            Face::LiberationSans => data::LIBERATION_SANS_LINE_HEIGHT,
            Face::LiberationSerif => data::LIBERATION_SERIF_LINE_HEIGHT,
            Face::LiberationMono => data::LIBERATION_MONO_LINE_HEIGHT,
            Face::Gelasio => data::GELASIO_LINE_HEIGHT,
        }
    }

    /// The advance of one character in thousandths of an em. Characters
    /// outside Latin-1 take the width of `n`, or a full em for East Asian
    /// scripts.
    fn advance(self, bold: bool, ch: char) -> u32 {
        let table = self.table(bold);
        let code = ch as u32;
        if code < 32 {
            return 0;
        }
        if code < 256 {
            return table[(code - 32) as usize] as u32;
        }
        let east_asian = (0x1100..=0x11FF).contains(&code)
            || (0x2E80..=0xA4CF).contains(&code)
            || (0xAC00..=0xD7A3).contains(&code)
            || (0xF900..=0xFAFF).contains(&code)
            || (0xFF00..=0xFF60).contains(&code)
            || (0x20000..=0x3FFFF).contains(&code);
        match east_asian {
            true => 1000,
            false => table[(b'n' - 32) as usize] as u32,
        }
    }
}

/// One font as a run uses it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Font {
    pub(crate) face: Face,
    pub(crate) bold: bool,
    /// Size in points.
    pub(crate) size: u32,
}

impl Font {
    /// The advance of one character in thousandths of a point.
    fn advance(self, ch: char) -> i64 {
        self.face.advance(self.bold, ch) as i64 * self.size as i64
    }

    /// The width of `text` set on one line, in EMU.
    #[cfg(test)]
    pub(crate) fn width(self, text: &str) -> i64 {
        let thousandths: i64 = text.chars().map(|ch| self.advance(ch)).sum();
        thousandths * EMU_PER_POINT / 1000
    }

    /// The height of one line at `spacing` percent, in EMU.
    pub(crate) fn line_height(self, spacing: u32) -> i64 {
        self.face.line_height() as i64 * self.size as i64 * EMU_PER_POINT * spacing as i64 / 100_000
    }
}

/// A word or a stretch of spaces with its width in thousandths of a point.
struct Token {
    width: i64,
    space: bool,
    /// Per-character widths, for breaking a word wider than the line.
    chars: Vec<i64>,
}

fn tokens(paragraph: &Paragraph, resolve: &dyn Fn(&crate::Run) -> Font) -> Vec<Token> {
    let mut out: Vec<Token> = Vec::new();
    for run in &paragraph.runs {
        let font = resolve(run);
        for ch in run.text.chars() {
            let space = ch.is_whitespace();
            let width = font.advance(ch);
            match out.last_mut() {
                Some(last) if last.space == space => {
                    last.width += width;
                    last.chars.push(width);
                }
                _ => out.push(Token {
                    width,
                    space,
                    chars: vec![width],
                }),
            }
        }
    }
    out
}

/// How many lines `paragraph` takes when wrapped at `width` EMU, never
/// fewer than one. Words break at spaces; a word wider than the line
/// breaks between characters.
pub(crate) fn line_count(
    paragraph: &Paragraph,
    width: i64,
    resolve: &dyn Fn(&crate::Run) -> Font,
) -> usize {
    let width = (width.max(1) * 1000 + EMU_PER_POINT - 1) / EMU_PER_POINT;
    let mut lines = 1usize;
    let mut used = 0i64;
    for token in tokens(paragraph, resolve) {
        if token.space {
            if used > 0 {
                used += token.width;
            }
            continue;
        }
        if used > 0 && used + token.width > width {
            lines += 1;
            used = 0;
        }
        if token.width <= width {
            used += token.width;
            continue;
        }
        for ch in token.chars {
            if used > 0 && used + ch > width {
                lines += 1;
                used = 0;
            }
            used += ch;
        }
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Run;

    fn calibri(size: u32) -> Font {
        Font {
            face: Face::Carlito,
            bold: false,
            size,
        }
    }

    #[test]
    fn faces_stand_in_for_font_names() {
        assert_eq!(Face::for_font("Calibri"), Face::Carlito);
        assert_eq!(Face::for_font("Calibri Light"), Face::Carlito);
        assert_eq!(Face::for_font("Arial"), Face::LiberationSans);
        assert_eq!(Face::for_font("Inter"), Face::LiberationSans);
        assert_eq!(Face::for_font("Georgia"), Face::Gelasio);
        assert_eq!(Face::for_font("Cambria"), Face::Caladea);
        assert_eq!(Face::for_font("Times New Roman"), Face::LiberationSerif);
        assert_eq!(Face::for_font("Source Serif Pro"), Face::LiberationSerif);
        assert_eq!(Face::for_font("Noto Sans Serif"), Face::LiberationSans);
        assert_eq!(Face::for_font("Menlo"), Face::LiberationMono);
        assert_eq!(Face::for_font("Fira Code"), Face::LiberationMono);
    }

    #[test]
    fn widths_scale_with_size_and_weight() {
        let n = Font {
            face: Face::Carlito,
            bold: false,
            size: 10,
        }
        .width("n");
        assert_eq!(n, 525 * 10 * EMU_PER_POINT / 1000);
        assert_eq!(calibri(20).width("n"), 2 * n);
        assert!(
            Font {
                face: Face::Carlito,
                bold: true,
                size: 10
            }
            .width("n")
                > n
        );
        assert_eq!(calibri(10).width("\u{1}"), 0);
        assert_eq!(calibri(10).width("漢"), 10 * EMU_PER_POINT);
        assert_eq!(calibri(10).width("ā"), n);
        assert_eq!(
            calibri(28).line_height(90),
            1221 * 28 * EMU_PER_POINT * 90 / 100_000
        );
    }

    #[test]
    fn lines_wrap_at_spaces_and_break_long_words() {
        let resolve = |_: &Run| calibri(28);
        let para = Paragraph::text("one two three four");
        let full = calibri(28).width("one two three four");
        assert_eq!(line_count(&para, full, &resolve), 1);
        assert_eq!(line_count(&para, full - EMU_PER_POINT, &resolve), 2);
        let one = calibri(28).width("three");
        assert_eq!(line_count(&para, one, &resolve), 4);
        assert_eq!(line_count(&Paragraph::text(""), 1000, &resolve), 1);
        let glued = Paragraph::text("abcdefghij");
        let two_chars = calibri(28).width("ab");
        assert_eq!(line_count(&glued, two_chars, &resolve), 5);
        let across_runs = Paragraph::runs(vec![Run::text("ab"), Run::text("cd ef")]);
        assert_eq!(
            line_count(&across_runs, calibri(28).width("abcd"), &resolve),
            2
        );
        assert_eq!(
            line_count(&Paragraph::text("   lead"), EMU_PER_POINT, &resolve),
            4
        );
    }
}
