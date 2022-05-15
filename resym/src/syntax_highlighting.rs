use eframe::{egui, epaint::text::LayoutJob};
use syntect::{easy::HighlightLines, highlighting::FontStyle, util::LinesWithEndings};

use resym_core::syntax_highlighting::CodeTheme;

/// Memoized code highlighting
pub fn highlight_code(
    ctx: &egui::Context,
    theme: &CodeTheme,
    code: &str,
    language: &str,
) -> LayoutJob {
    impl egui::util::cache::ComputerMut<(&CodeTheme, &str, &str), LayoutJob> for CodeHighlighter {
        fn compute(&mut self, (theme, code, lang): (&CodeTheme, &str, &str)) -> LayoutJob {
            self.highlight(theme, code, lang)
        }
    }

    type HighlightCache<'a> = egui::util::cache::FrameCache<LayoutJob, CodeHighlighter>;

    let mut memory = ctx.memory();
    let highlight_cache = memory.caches.cache::<HighlightCache<'_>>();
    highlight_cache.get((theme, code, language))
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
    fn highlight(&self, theme: &CodeTheme, code: &str, lang: &str) -> LayoutJob {
        self.highlight_impl(theme, code, lang).unwrap_or_else(|| {
            // Fallback:
            LayoutJob::simple(
                code.into(),
                egui::FontId::monospace(14.0),
                if theme.dark_mode {
                    egui::Color32::LIGHT_GRAY
                } else {
                    egui::Color32::DARK_GRAY
                },
                f32::INFINITY,
            )
        })
    }

    fn highlight_impl(&self, theme: &CodeTheme, text: &str, language: &str) -> Option<LayoutJob> {
        const COLOR_TRANSPARENT: egui::Color32 = egui::Color32::from_rgb_additive(0, 0, 0);
        const COLOR_RED: egui::Color32 = egui::Color32::from_rgb(0x50, 0x10, 0x10);
        const COLOR_GREEN: egui::Color32 = egui::Color32::from_rgb(0x10, 0x50, 0x10);

        let syntax = self
            .ps
            .find_syntax_by_name(language)
            .or_else(|| self.ps.find_syntax_by_extension(language))?;

        let theme = theme.syntect_theme.syntect_key_name();
        let mut h = HighlightLines::new(syntax, &self.ts.themes[theme]);

        use egui::text::{LayoutSection, TextFormat};

        let mut job = LayoutJob {
            text: text.into(),
            ..Default::default()
        };

        for line in LinesWithEndings::from(text) {
            let mut bg_color = egui::Color32::TRANSPARENT;
            for (style, range) in h.highlight_line(line, &self.ps).ok()? {
                // Change the background of regions that have been affected in the diff.
                // FIXME: This is really dirty, do better.
                if range == "+" {
                    bg_color = COLOR_GREEN;
                } else if range == "-" {
                    bg_color = COLOR_RED;
                } else if range == "\n" {
                    bg_color = COLOR_TRANSPARENT;
                }

                let fg = style.foreground;
                let text_color = egui::Color32::from_rgb(fg.r, fg.g, fg.b);
                let italics = style.font_style.contains(FontStyle::ITALIC);
                let underline = style.font_style.contains(FontStyle::ITALIC);
                let underline = if underline {
                    egui::Stroke::new(1.0, text_color)
                } else {
                    egui::Stroke::none()
                };
                job.sections.push(LayoutSection {
                    leading_space: 0.0,
                    byte_range: as_byte_range(text, range),
                    format: TextFormat {
                        background: bg_color,
                        font_id: egui::FontId::monospace(14.0),
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

fn as_byte_range(whole: &str, range: &str) -> std::ops::Range<usize> {
    let whole_start = whole.as_ptr() as usize;
    let range_start = range.as_ptr() as usize;
    assert!(whole_start <= range_start);
    assert!(range_start + range.len() <= whole_start + whole.len());
    let offset = range_start - whole_start;
    offset..(offset + range.len())
}
