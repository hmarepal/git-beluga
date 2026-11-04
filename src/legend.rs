//! A small image explaining the whale's colors: one swatch, name and percentage per language.

use std::fmt::Write;

use crate::activity::Language;
use crate::whale::Rgb;

const WIDTH: usize = 512;
const COLUMNS: usize = 2;
const ROW_HEIGHT: usize = 28;
const SWATCH: usize = 14;

fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// Text color follows the viewer's GitHub theme. Languages fill the columns top to bottom,
/// biggest share first, in the order the rings run from head to tail.
pub fn svg(languages: &[Language], colors: &[Rgb]) -> String {
    let rows = languages.len().div_ceil(COLUMNS).max(1);
    let height = rows * ROW_HEIGHT;
    let column_width = WIDTH / COLUMNS;
    let mut out = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{WIDTH}\" height=\"{height}\" viewBox=\"0 0 {WIDTH} {height}\">\n\
         <style>text{{font:14px -apple-system,BlinkMacSystemFont,'Segoe UI',Helvetica,Arial,sans-serif;fill:#1f2328}}\
         .share{{font-variant-numeric:tabular-nums;font-weight:600}}\
         @media (prefers-color-scheme:dark){{text{{fill:#e6edf3}}}}</style>\n"
    );
    for (i, (language, color)) in languages.iter().zip(colors).enumerate() {
        let (column, row) = (i / rows, i % rows);
        let x = column * column_width + 16;
        let y = row * ROW_HEIGHT;
        let middle = y + ROW_HEIGHT / 2;
        let _ = writeln!(
            out,
            "<rect x=\"{x}\" y=\"{}\" width=\"{SWATCH}\" height=\"{SWATCH}\" fill=\"#{:02x}{:02x}{:02x}\"/>",
            middle - SWATCH / 2,
            color.0,
            color.1,
            color.2,
        );
        let _ = writeln!(
            out,
            "<text x=\"{}\" y=\"{}\">{}</text><text class=\"share\" x=\"{}\" y=\"{}\" text-anchor=\"end\">{:.1}%</text>",
            x + SWATCH + 8,
            middle + 5,
            escape(&language.name),
            (column + 1) * column_width - 16,
            middle + 5,
            language.share * 100.0,
        );
    }
    out.push_str("</svg>\n");
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn language(name: &str, share: f32) -> Language {
        Language {
            name: name.into(),
            color: "#000000".into(),
            share,
        }
    }

    #[test]
    fn lists_every_language_with_its_percentage_and_color() {
        let text = svg(
            &[language("Cuda", 0.83), language("C++", 0.17)],
            &[Rgb(0x11, 0x22, 0x33), Rgb(0xaa, 0xbb, 0xcc)],
        );
        assert!(text.contains(">Cuda<") && text.contains(">83.0%<") && text.contains("#112233"));
        assert!(text.contains(">C++<") && text.contains(">17.0%<") && text.contains("#aabbcc"));
    }

    #[test]
    fn language_names_cannot_break_the_image() {
        let text = svg(&[language("A&B<C>", 1.0)], &[Rgb(0, 0, 0)]);
        assert!(text.contains(">A&amp;B&lt;C&gt;<"));
    }

    #[test]
    fn no_languages_still_makes_a_valid_image() {
        assert!(svg(&[], &[]).ends_with("</svg>\n"));
    }
}
