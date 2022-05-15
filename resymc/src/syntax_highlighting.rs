use resym_core::syntax_highlighting::CodeTheme;
use syntect::{
    easy::HighlightLines,
    highlighting::{Color, Style},
    util::{as_24_bit_terminal_escaped, LinesWithEndings},
};

/// Function relying on `syntect` to highlight the given `code` str.
/// In case of success, the result is a `String` that is ready to be printed in a
/// terminal.
pub fn highlight_code(theme: &CodeTheme, code: &str, language: &str) -> Option<String> {
    let highlighter = CodeHighlighter::default();
    highlighter.highlight(theme, code, language)
}

struct CodeHighlighter {
    ps: syntect::parsing::SyntaxSet,
    ts: syntect::highlighting::ThemeSet,
}

impl Default for CodeHighlighter {
    fn default() -> Self {
        Self {
            ps: syntect::parsing::SyntaxSet::load_defaults_newlines(),
            ts: syntect::highlighting::ThemeSet::load_defaults(),
        }
    }
}

impl CodeHighlighter {
    fn highlight(&self, theme: &CodeTheme, code: &str, language: &str) -> Option<String> {
        use std::fmt::Write;

        let syntax = self
            .ps
            .find_syntax_by_name(language)
            .or_else(|| self.ps.find_syntax_by_extension(language))?;

        let theme = theme.syntect_theme.syntect_key_name();
        let mut output = String::default();
        let mut h = HighlightLines::new(syntax, &self.ts.themes[theme]);
        for line in LinesWithEndings::from(code) {
            let mut regions = h.highlight_line(line, &self.ps).ok()?;
            hightlight_regions_diff(&mut regions);
            let _r = write!(
                &mut output,
                "{}",
                as_24_bit_terminal_escaped(&regions[..], true)
            );
        }

        Some(output)
    }
}

/// Changes the background of regions that have been affected in the diff.
// FIXME: This is really dirty, do better.
fn hightlight_regions_diff(regions: &mut Vec<(Style, &str)>) {
    const COLOR_TRANSPARENT: Color = Color {
        r: 0x00,
        g: 0x00,
        b: 0x00,
        a: 0x00,
    };
    const COLOR_RED: Color = Color {
        r: 0x50,
        g: 0x10,
        b: 0x10,
        a: 0xFF,
    };
    const COLOR_GREEN: Color = Color {
        r: 0x10,
        g: 0x50,
        b: 0x10,
        a: 0xFF,
    };

    let mut bg_color = COLOR_TRANSPARENT;
    regions.iter_mut().for_each(|(style, s)| {
        if *s == "+" {
            bg_color = COLOR_GREEN;
        } else if *s == "-" {
            bg_color = COLOR_RED;
        } else if *s == "\n" {
            bg_color = COLOR_TRANSPARENT;
        }
        style.background = bg_color;
    });
}
