use resym_core::{diffing::DiffChange, syntax_highlighting::CodeTheme};
use syntect::{
    easy::HighlightLines,
    highlighting::{Color, Style},
    util::{as_24_bit_terminal_escaped, LinesWithEndings},
};

pub type LineDescriptions = Vec<DiffChange>;

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

/// Function relying on `syntect` to highlight the given `code` str.
/// In case of success, the result is a `String` that is ready to be printed in a
/// terminal.
pub fn highlight_code(
    theme: &CodeTheme,
    code: &str,
    line_descriptions: Option<LineDescriptions>,
) -> Option<String> {
    let highlighter = CodeHighlighter::default();
    highlighter.highlight(theme, code, &theme.language_syntax, line_descriptions)
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
    fn highlight(
        &self,
        theme: &CodeTheme,
        code: &str,
        language: &str,
        line_descriptions: Option<LineDescriptions>,
    ) -> Option<String> {
        use std::fmt::Write;

        let syntax = self
            .ps
            .find_syntax_by_name(language)
            .or_else(|| self.ps.find_syntax_by_extension(language))?;

        let theme = theme.syntect_theme.syntect_key_name();
        let mut output = String::default();
        let mut h = HighlightLines::new(syntax, &self.ts.themes[theme]);
        for (line_id, line) in LinesWithEndings::from(code).enumerate() {
            let mut regions = h.highlight_line(line, &self.ps).ok()?;
            // Apply highlight related to diff changes if needed
            if let Some(line_descriptions) = &line_descriptions {
                highlight_regions_diff(&mut regions, line_descriptions.get(line_id));
            } else {
                highlight_regions_diff(&mut regions, None);
            }
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
fn highlight_regions_diff(regions: &mut [(Style, &str)], line_description: Option<&DiffChange>) {
    if let Some(line_description) = line_description {
        let bg_color = match line_description {
            DiffChange::Insert => COLOR_GREEN,
            DiffChange::Delete => COLOR_RED,
            DiffChange::Equal => COLOR_TRANSPARENT,
        };
        regions.iter_mut().for_each(|(style, str)| {
            if *str != "\n" {
                style.background = bg_color;
            } else {
                style.background = COLOR_TRANSPARENT;
            }
        });
    } else {
        regions.iter_mut().for_each(|(style, _)| {
            style.background = COLOR_TRANSPARENT;
        });
    }
}
