#[derive(Clone, Copy, Hash, PartialEq, Eq)]
pub enum SyntectTheme {
    Base16EightiesDark,
    Base16MochaDark,
    Base16OceanDark,
    Base16OceanLight,
    InspiredGitHub,
    SolarizedDark,
    SolarizedLight,
}

impl SyntectTheme {
    pub fn syntect_key_name(&self) -> &'static str {
        match self {
            Self::Base16EightiesDark => "base16-eighties.dark",
            Self::Base16MochaDark => "base16-mocha.dark",
            Self::Base16OceanDark => "base16-ocean.dark",
            Self::Base16OceanLight => "base16-ocean.light",
            Self::InspiredGitHub => "InspiredGitHub",
            Self::SolarizedDark => "Solarized (dark)",
            Self::SolarizedLight => "Solarized (light)",
        }
    }

    pub fn is_dark(&self) -> bool {
        match self {
            Self::Base16EightiesDark
            | Self::Base16MochaDark
            | Self::Base16OceanDark
            | Self::SolarizedDark => true,

            Self::Base16OceanLight | Self::InspiredGitHub | Self::SolarizedLight => false,
        }
    }
}

#[derive(Clone, Hash, PartialEq, Eq)]
pub struct CodeTheme {
    pub dark_mode: bool,
    pub syntect_theme: SyntectTheme,
    pub font_size: u16,
    pub language_syntax: String,
}

impl Default for CodeTheme {
    fn default() -> Self {
        Self::dark(14, "cpp".to_string())
    }
}

impl CodeTheme {
    pub fn dark(font_size: u16, language_syntax: String) -> Self {
        Self {
            dark_mode: true,
            syntect_theme: SyntectTheme::Base16MochaDark,
            font_size,
            language_syntax,
        }
    }

    pub fn light(font_size: u16, language_syntax: String) -> Self {
        Self {
            dark_mode: false,
            syntect_theme: SyntectTheme::Base16OceanLight,
            font_size,
            language_syntax,
        }
    }
}
