use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::{fmt, path::PathBuf, str::FromStr};

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Region {
    Eu,
    Us,
    Kr,
    Cn,
}

impl Region {
    pub const ALL: [Region; 4] = [Region::Us, Region::Eu, Region::Kr, Region::Cn];

    pub fn as_str(self) -> &'static str {
        match self {
            Region::Eu => "eu",
            Region::Us => "us",
            Region::Kr => "kr",
            Region::Cn => "cn",
        }
    }

    pub fn remote_url(self) -> String {
        format!("http://{}.patch.battle.net:1119/hsb", self.as_str())
    }

    pub fn default_cdn(self) -> &'static str {
        match self {
            Region::Cn => "https://blzdist-hs.necdn.leihuo.netease.com/tpr/hs",
            _ => "http://level3.blizzard.com/tpr/hs",
        }
    }

    pub fn aurora_env(self) -> &'static str {
        match self {
            Region::Cn => "cn.actual.battlenet.com.cn",
            Region::Eu => "eu.actual.battle.net",
            Region::Us => "us.actual.battle.net",
            Region::Kr => "kr.actual.battle.net",
        }
    }

    pub fn login_url(self) -> &'static str {
        match self {
            Region::Cn => "https://account.battlenet.com.cn/login/zh/?ref=blizzard-hearthstone://localhost:0/&app=wtcg-and&showCredentials=true",
            _ => "https://battle.net/login/?app=wtcg",
        }
    }
}

impl fmt::Display for Region {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for Region {
    type Err = anyhow::Error;

    fn from_str(value: &str) -> Result<Self> {
        match value {
            "eu" => Ok(Region::Eu),
            "us" => Ok(Region::Us),
            "kr" => Ok(Region::Kr),
            "cn" => Ok(Region::Cn),
            _ => anyhow::bail!("unsupported region `{value}`"),
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum Locale {
    #[serde(rename = "deDE")]
    DeDe,
    #[serde(rename = "enGB")]
    EnGb,
    #[serde(rename = "enUS")]
    EnUs,
    #[serde(rename = "esES")]
    EsEs,
    #[serde(rename = "esMX")]
    EsMx,
    #[serde(rename = "frFR")]
    FrFr,
    #[serde(rename = "itIT")]
    ItIt,
    #[serde(rename = "jaJP")]
    JaJp,
    #[serde(rename = "koKR")]
    KoKr,
    #[serde(rename = "plPL")]
    PlPl,
    #[serde(rename = "ptBR")]
    PtBr,
    #[serde(rename = "ruRU")]
    RuRu,
    #[serde(rename = "thTH")]
    ThTh,
    #[serde(rename = "zhCN")]
    ZhCn,
    #[serde(rename = "zhTW")]
    ZhTw,
}

impl Locale {
    pub const ALL: [Locale; 15] = [
        Locale::DeDe,
        Locale::EnGb,
        Locale::EnUs,
        Locale::EsEs,
        Locale::EsMx,
        Locale::FrFr,
        Locale::ItIt,
        Locale::JaJp,
        Locale::KoKr,
        Locale::PlPl,
        Locale::PtBr,
        Locale::RuRu,
        Locale::ThTh,
        Locale::ZhCn,
        Locale::ZhTw,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Locale::DeDe => "deDE",
            Locale::EnGb => "enGB",
            Locale::EnUs => "enUS",
            Locale::EsEs => "esES",
            Locale::EsMx => "esMX",
            Locale::FrFr => "frFR",
            Locale::ItIt => "itIT",
            Locale::JaJp => "jaJP",
            Locale::KoKr => "koKR",
            Locale::PlPl => "plPL",
            Locale::PtBr => "ptBR",
            Locale::RuRu => "ruRU",
            Locale::ThTh => "thTH",
            Locale::ZhCn => "zhCN",
            Locale::ZhTw => "zhTW",
        }
    }
}

impl fmt::Display for Locale {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for Locale {
    type Err = anyhow::Error;

    fn from_str(value: &str) -> Result<Self> {
        Locale::ALL
            .into_iter()
            .find(|locale| locale.as_str() == value)
            .with_context(|| format!("unsupported locale `{value}`"))
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct AppConfig {
    pub region: Region,
    pub locale: Locale,
    pub game_dir: Option<PathBuf>,
    pub installed_version: Option<String>,
    pub unity_version: Option<String>,
    #[serde(default)]
    pub logged_in: bool,
    #[serde(default)]
    pub last_login_at: Option<String>,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            region: Region::Us,
            locale: Locale::EnUs,
            game_dir: None,
            installed_version: None,
            unity_version: None,
            logged_in: false,
            last_login_at: None,
        }
    }
}

impl AppConfig {
    pub fn load_or_default(path: &std::path::Path) -> Result<Self> {
        if !path.exists() {
            return Ok(Self::default());
        }

        let data = std::fs::read_to_string(path)
            .with_context(|| format!("failed to read {}", path.display()))?;
        toml::from_str(&data).with_context(|| format!("failed to parse {}", path.display()))
    }

    pub fn save(&self, path: &std::path::Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let data = toml::to_string_pretty(self)?;
        std::fs::write(path, data).with_context(|| format!("failed to write {}", path.display()))
    }
}
