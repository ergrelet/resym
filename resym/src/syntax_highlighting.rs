use eframe::{
    egui,
    epaint::text::{LayoutJob, TextWrapping},
};
use syntect::{easy::HighlightLines, highlighting::FontStyle, util::LinesWithEndings};

use resym_core::{diffing::DiffChange, syntax_highlighting::CodeTheme};

pub type LineDescriptions = Vec<DiffChange>;

/// Memoized code highlighting
pub fn highlight_code(
    ctx: &egui::Context,
    theme: &CodeTheme,
    code: &str,
    enabled: bool,
    line_descriptions: Option<&LineDescriptions>,
) -> LayoutJob {
    type HighlightCache<'a> = egui::util::cache::FrameCache<LayoutJob, CodeHighlighter>;

    ctx.memory_mut(|memory| {
        let highlight_cache = memory.caches.cache::<HighlightCache<'_>>();
        highlight_cache.get((theme, code, enabled, line_descriptions))
    })
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
        enabled: bool,
        line_descriptions: Option<&LineDescriptions>,
    ) -> LayoutJob {
        self.highlight_impl(theme, code, enabled, line_descriptions)
            .unwrap_or_else(|| {
                // Fallback:
                LayoutJob::simple(
                    code.into(),
                    egui::FontId::monospace(theme.font_size as f32),
                    if theme.dark_mode {
                        egui::Color32::LIGHT_GRAY
                    } else {
                        egui::Color32::DARK_GRAY
                    },
                    f32::INFINITY,
                )
            })
    }

    fn highlight_impl(
        &self,
        theme: &CodeTheme,
        text: &str,
        enabled: bool,
        line_descriptions: Option<&LineDescriptions>,
    ) -> Option<LayoutJob> {
        if !enabled {
            return None;
        }

        const COLOR_RED: egui::Color32 = egui::Color32::from_rgb(0x50, 0x10, 0x10);
        const COLOR_GREEN: egui::Color32 = egui::Color32::from_rgb(0x10, 0x50, 0x10);

        let syntax = self
            .ps
            .find_syntax_by_name(&theme.language_syntax)
            .or_else(|| self.ps.find_syntax_by_extension(&theme.language_syntax))?;

        let theme_name = theme.syntect_theme.syntect_key_name();
        let mut h = HighlightLines::new(syntax, &self.ts.themes[theme_name]);

        use egui::text::{LayoutSection, TextFormat};

        let mut job = LayoutJob {
            text: text.into(),
            // Disable wrapping forcefully
            wrap: TextWrapping {
                max_width: f32::INFINITY,
                ..Default::default()
            },
            ..Default::default()
        };

        for (line_id, line) in LinesWithEndings::from(text).enumerate() {
            // Change the background of regions that have been affected in the diff.
            let bg_color = match line_descriptions {
                None => egui::Color32::TRANSPARENT,
                Some(line_desc) => match line_desc.get(line_id) {
                    None => egui::Color32::TRANSPARENT,
                    Some(line_desc) => match line_desc {
                        DiffChange::Insert => COLOR_GREEN,
                        DiffChange::Delete => COLOR_RED,
                        DiffChange::Equal => egui::Color32::TRANSPARENT,
                    },
                },
            };

            for (style, range) in h.highlight_line(line, &self.ps).ok()? {
                let fg = style.foreground;
                let text_color = egui::Color32::from_rgb(fg.r, fg.g, fg.b);
                let italics = style.font_style.contains(FontStyle::ITALIC);
                let underline = style.font_style.contains(FontStyle::ITALIC);
                let underline = if underline {
                    egui::Stroke::new(1.0, text_color)
                } else {
                    egui::Stroke::NONE
                };
                job.sections.push(LayoutSection {
                    leading_space: 0.0,
                    byte_range: as_byte_range(text, range),
                    format: TextFormat {
                        background: bg_color,
                        font_id: egui::FontId::monospace(theme.font_size as f32),
                        color: text_color,
                        italics,
                        underline,
                        ..Default::default()
                    },
                });
            }
        }

        Some(job)
    }
}

impl egui::util::cache::ComputerMut<(&CodeTheme, &str, bool, Option<&LineDescriptions>), LayoutJob>
    for CodeHighlighter
{
    fn compute(
        &mut self,
        (theme, code, enabled, line_descriptions): (
            &CodeTheme,
            &str,
            bool,
            Option<&LineDescriptions>,
        ),
    ) -> LayoutJob {
        self.highlight(theme, code, enabled, line_descriptions)
    }
}

fn as_byte_range(whole: &str, range: &str) -> std::ops::Range<usize> {
    let whole_start = whole.as_ptr() as usize;
    let range_start = range.as_ptr() as usize;
    assert!(whole_start <= range_start);
    assert!(range_start + range.len() <= whole_start + whole.len());
    let offset = range_start - whole_start;
    offset..(offset + range.len())
}
