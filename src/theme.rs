use std::path::PathBuf;
use std::{fs, io};

use eframe::egui::Color32;
use serde::{Deserialize, Deserializer, de::Error};

const DEFAULT: &str = include_str!("../assets/evergarden.toml");

#[derive(Deserialize)]
pub struct Theme {
    #[serde(deserialize_with = "color")]
    pub background: Color32,
    #[serde(deserialize_with = "color")]
    pub surface: Color32,
    #[serde(deserialize_with = "color")]
    pub text: Color32,
    #[serde(deserialize_with = "color")]
    pub muted: Color32,
    #[serde(deserialize_with = "color")]
    pub border: Color32,
    #[serde(deserialize_with = "color")]
    pub on_accent: Color32,
    #[serde(deserialize_with = "color")]
    pub selected: Color32,
    #[serde(deserialize_with = "color")]
    pub error: Color32,
    #[serde(deserialize_with = "colors")]
    pub accents: Vec<Color32>,
}

fn parse<E: Error>(hex: &str) -> Result<Color32, E> {
    Color32::from_hex(hex).map_err(|e| E::custom(format!("invalid color {hex:?}: {e:?}")))
}

fn color<'de, D: Deserializer<'de>>(d: D) -> Result<Color32, D::Error> {
    parse(&String::deserialize(d)?)
}

fn colors<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<Color32>, D::Error> {
    let colors = Vec::<String>::deserialize(d)?;
    if colors.is_empty() {
        return Err(D::Error::custom("accents must not be empty"));
    }
    colors.iter().map(|c| parse(c)).collect()
}

pub fn path() -> Option<PathBuf> {
    let config = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))?;
    Some(config.join("klemmbrett").join("theme.toml"))
}

impl Default for Theme {
    fn default() -> Self {
        toml::from_str(DEFAULT).expect("default theme is valid")
    }
}

impl Theme {
    pub fn load() -> (Self, Option<String>) {
        let Some(path) = path() else { return (Self::default(), None) };
        let user = match fs::read_to_string(&path) {
            Ok(user) => user,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return (Self::default(), None),
            Err(e) => return (Self::default(), Some(format!("failed to read {}: {e}", path.display()))),
        };

        let mut table: toml::Table = toml::from_str(DEFAULT).expect("default theme is valid");
        let merged = toml::from_str::<toml::Table>(&user).and_then(|user| {
            table.extend(user);
            toml::Value::Table(table).try_into()
        });
        match merged {
            Ok(theme) => (theme, None),
            Err(e) => (Self::default(), Some(format!("invalid theme {}: {e}", path.display()))),
        }
    }

    pub fn accent(&self, idx: usize) -> Color32 {
        self.accents[idx % self.accents.len()]
    }

    pub fn is_dark(&self) -> bool {
        let [r, g, b, _] = self.background.to_array();
        (0.299 * r as f32 + 0.587 * g as f32 + 0.114 * b as f32) < 128.0
    }
}
