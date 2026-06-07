use std::io::Read;
use std::path::{Path, PathBuf};

use flate2::read::GzDecoder;
use serde::{Deserialize, Serialize};

use crate::error::AppError;
use crate::instances::Instance;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorldEntry {
    pub folder_name: String,
    pub display_name: String,
    pub path: PathBuf,
    pub icon: Option<PathBuf>,
    pub game_mode: Option<WorldGameMode>,
    pub hardcore: bool,
    pub cheats: Option<bool>,
    pub difficulty: Option<String>,
    pub last_played_unix: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum WorldGameMode {
    Survival,
    Creative,
    Adventure,
    Spectator,
    Unknown(i32),
}

impl WorldGameMode {
    pub fn label(self) -> String {
        match self {
            Self::Survival => "Survival".into(),
            Self::Creative => "Creative".into(),
            Self::Adventure => "Adventure".into(),
            Self::Spectator => "Spectator".into(),
            Self::Unknown(value) => format!("Mode {value}"),
        }
    }

    fn from_id(value: i32) -> Self {
        match value {
            0 => Self::Survival,
            1 => Self::Creative,
            2 => Self::Adventure,
            3 => Self::Spectator,
            other => Self::Unknown(other),
        }
    }
}

#[derive(Debug, Clone, Default)]
struct WorldMetadata {
    display_name: Option<String>,
    game_mode: Option<WorldGameMode>,
    hardcore: bool,
    cheats: Option<bool>,
    difficulty: Option<String>,
    last_played_unix: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerEntry {
    pub name: String,
    pub address: String,
    pub icon: Option<Vec<u8>>,
}

pub async fn list_worlds(instance: Instance) -> Result<Vec<WorldEntry>, AppError> {
    tokio::task::spawn_blocking(move || list_worlds_blocking(&instance))
        .await
        .map_err(|error| AppError::Process(error.to_string()))?
}

pub async fn list_servers(instance: Instance) -> Result<Vec<ServerEntry>, AppError> {
    tokio::task::spawn_blocking(move || list_servers_blocking(&instance))
        .await
        .map_err(|error| AppError::Process(error.to_string()))?
}

pub fn worlds_dir(instance: &Instance) -> PathBuf {
    effective_game_dir(instance).join("saves")
}

pub fn servers_file(instance: &Instance) -> PathBuf {
    effective_game_dir(instance).join("servers.dat")
}

fn list_worlds_blocking(instance: &Instance) -> Result<Vec<WorldEntry>, AppError> {
    let saves = worlds_dir(instance);
    if !saves.exists() {
        return Ok(Vec::new());
    }

    let mut worlds = Vec::new();
    for entry in std::fs::read_dir(&saves).map_err(|error| AppError::Storage(error.to_string()))? {
        let entry = entry.map_err(|error| AppError::Storage(error.to_string()))?;
        let path = entry.path();
        if !path.is_dir() || !path.join("level.dat").exists() {
            continue;
        }
        let folder_name = entry.file_name().to_string_lossy().to_string();
        let metadata = read_world_metadata(&path).unwrap_or_default();
        let display_name = metadata
            .display_name
            .clone()
            .unwrap_or_else(|| folder_name.clone());
        let icon = path.join("icon.png");
        let icon = icon.exists().then_some(icon);
        worlds.push(WorldEntry {
            folder_name,
            display_name,
            path,
            icon,
            game_mode: metadata.game_mode,
            hardcore: metadata.hardcore,
            cheats: metadata.cheats,
            difficulty: metadata.difficulty,
            last_played_unix: metadata.last_played_unix,
        });
    }
    worlds.sort_by(|a, b| a.display_name.cmp(&b.display_name));
    Ok(worlds)
}

fn list_servers_blocking(instance: &Instance) -> Result<Vec<ServerEntry>, AppError> {
    let path = servers_file(instance);
    if !path.exists() {
        return Ok(Vec::new());
    }
    let bytes = std::fs::read(path).map_err(|error| AppError::Storage(error.to_string()))?;
    Ok(read_servers_dat(&bytes).unwrap_or_default())
}

fn effective_game_dir(instance: &Instance) -> PathBuf {
    if instance.game_dir_override.trim().is_empty() {
        instance.path.clone()
    } else {
        PathBuf::from(instance.game_dir_override.trim())
    }
}

fn read_world_metadata(path: &Path) -> Option<WorldMetadata> {
    let bytes = std::fs::read(path.join("level.dat")).ok()?;
    let bytes = maybe_decompress_gzip(bytes).ok()?;
    read_level_metadata(&bytes)
}

fn maybe_decompress_gzip(bytes: Vec<u8>) -> Result<Vec<u8>, AppError> {
    if bytes.starts_with(&[0x1f, 0x8b]) {
        let mut out = Vec::new();
        GzDecoder::new(bytes.as_slice())
            .read_to_end(&mut out)
            .map_err(|error| AppError::Storage(error.to_string()))?;
        Ok(out)
    } else {
        Ok(bytes)
    }
}

fn read_level_metadata(bytes: &[u8]) -> Option<WorldMetadata> {
    let mut reader = NbtReader::new(bytes);
    if reader.read_u8()? != TAG_COMPOUND {
        return None;
    }
    let _ = reader.read_string()?;
    let mut metadata = WorldMetadata::default();
    reader.scan_world_metadata_compound(&mut metadata)?;
    Some(metadata)
}

fn read_servers_dat(bytes: &[u8]) -> Option<Vec<ServerEntry>> {
    let mut reader = NbtReader::new(bytes);
    if reader.read_u8()? != TAG_COMPOUND {
        return None;
    }
    let _ = reader.read_string()?;
    reader.find_servers_list()
}

fn difficulty_label(value: u8) -> Option<String> {
    match value {
        0 => Some("Peaceful".into()),
        1 => Some("Easy".into()),
        2 => Some("Normal".into()),
        3 => Some("Hard".into()),
        other => Some(format!("Difficulty {other}")),
    }
}

const TAG_END: u8 = 0;
const TAG_BYTE: u8 = 1;
const TAG_SHORT: u8 = 2;
const TAG_INT: u8 = 3;
const TAG_LONG: u8 = 4;
const TAG_FLOAT: u8 = 5;
const TAG_DOUBLE: u8 = 6;
const TAG_BYTE_ARRAY: u8 = 7;
const TAG_STRING: u8 = 8;
const TAG_LIST: u8 = 9;
const TAG_COMPOUND: u8 = 10;
const TAG_INT_ARRAY: u8 = 11;
const TAG_LONG_ARRAY: u8 = 12;

struct NbtReader<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> NbtReader<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, pos: 0 }
    }

    fn read_u8(&mut self) -> Option<u8> {
        let value = *self.bytes.get(self.pos)?;
        self.pos += 1;
        Some(value)
    }

    fn read_u16(&mut self) -> Option<u16> {
        let bytes = self.bytes.get(self.pos..self.pos + 2)?;
        self.pos += 2;
        Some(u16::from_be_bytes([bytes[0], bytes[1]]))
    }

    fn read_i32(&mut self) -> Option<i32> {
        let bytes = self.bytes.get(self.pos..self.pos + 4)?;
        self.pos += 4;
        Some(i32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    }

    fn read_i64(&mut self) -> Option<i64> {
        let bytes = self.bytes.get(self.pos..self.pos + 8)?;
        self.pos += 8;
        Some(i64::from_be_bytes([
            bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
        ]))
    }

    fn read_string(&mut self) -> Option<String> {
        let len = self.read_u16()? as usize;
        let bytes = self.bytes.get(self.pos..self.pos + len)?;
        self.pos += len;
        String::from_utf8(bytes.to_vec()).ok()
    }

    fn skip(&mut self, len: usize) -> Option<()> {
        self.bytes.get(self.pos..self.pos + len)?;
        self.pos += len;
        Some(())
    }

    fn scan_world_metadata_compound(&mut self, metadata: &mut WorldMetadata) -> Option<()> {
        loop {
            let tag = self.read_u8()?;
            if tag == TAG_END {
                return Some(());
            }
            let name = self.read_string()?;
            match (tag, name.as_str()) {
                (TAG_STRING, "LevelName") => {
                    metadata.display_name = Some(self.read_string()?);
                }
                (TAG_INT, "GameType") => {
                    metadata.game_mode = Some(WorldGameMode::from_id(self.read_i32()?));
                }
                (TAG_BYTE, "hardcore") => {
                    metadata.hardcore = self.read_u8()? != 0;
                }
                (TAG_BYTE, "allowCommands") => {
                    metadata.cheats = Some(self.read_u8()? != 0);
                }
                (TAG_BYTE, "Difficulty") => {
                    metadata.difficulty = difficulty_label(self.read_u8()?);
                }
                (TAG_LONG, "LastPlayed") => {
                    let millis = self.read_i64()?;
                    metadata.last_played_unix = (millis > 0).then_some((millis as u64) / 1000);
                }
                (TAG_COMPOUND, _) => {
                    self.scan_world_metadata_compound(metadata)?;
                }
                _ => {
                    self.skip_payload(tag)?;
                }
            }
        }
    }

    fn find_servers_list(&mut self) -> Option<Vec<ServerEntry>> {
        loop {
            let tag = self.read_u8()?;
            if tag == TAG_END {
                return None;
            }
            let name = self.read_string()?;
            if tag == TAG_LIST && name == "servers" {
                return self.read_server_list();
            }
            self.skip_payload(tag)?;
        }
    }

    fn read_server_list(&mut self) -> Option<Vec<ServerEntry>> {
        let element_tag = self.read_u8()?;
        let len = self.read_i32()?.max(0) as usize;
        if element_tag != TAG_COMPOUND {
            for _ in 0..len {
                self.skip_payload(element_tag)?;
            }
            return Some(Vec::new());
        }
        let mut servers = Vec::with_capacity(len);
        for _ in 0..len {
            let mut name = String::new();
            let mut address = String::new();
            let mut icon = None;
            loop {
                let tag = self.read_u8()?;
                if tag == TAG_END {
                    break;
                }
                let key = self.read_string()?;
                if tag == TAG_STRING {
                    let value = self.read_string()?;
                    match key.as_str() {
                        "name" => name = value,
                        "ip" => address = value,
                        "icon" => icon = decode_server_icon(&value),
                        _ => {}
                    }
                } else {
                    self.skip_payload(tag)?;
                }
            }
            if !address.trim().is_empty() {
                servers.push(ServerEntry {
                    name: if name.trim().is_empty() {
                        address.clone()
                    } else {
                        name
                    },
                    address,
                    icon,
                });
            }
        }
        Some(servers)
    }

    fn skip_payload(&mut self, tag: u8) -> Option<()> {
        match tag {
            TAG_END => Some(()),
            TAG_BYTE => self.skip(1),
            TAG_SHORT => self.skip(2),
            TAG_INT | TAG_FLOAT => self.skip(4),
            TAG_LONG | TAG_DOUBLE => self.skip(8),
            TAG_BYTE_ARRAY => {
                let len = self.read_i32()?.max(0) as usize;
                self.skip(len)
            }
            TAG_STRING => self.read_string().map(|_| ()),
            TAG_LIST => {
                let tag = self.read_u8()?;
                let len = self.read_i32()?.max(0) as usize;
                for _ in 0..len {
                    self.skip_payload(tag)?;
                }
                Some(())
            }
            TAG_COMPOUND => {
                loop {
                    let tag = self.read_u8()?;
                    if tag == TAG_END {
                        break;
                    }
                    let _ = self.read_string()?;
                    self.skip_payload(tag)?;
                }
                Some(())
            }
            TAG_INT_ARRAY => {
                let len = self.read_i32()?.max(0) as usize;
                self.skip(len * 4)
            }
            TAG_LONG_ARRAY => {
                let len = self.read_i32()?.max(0) as usize;
                self.skip(len * 8)
            }
            _ => None,
        }
    }
}

fn decode_server_icon(value: &str) -> Option<Vec<u8>> {
    let payload = value
        .strip_prefix("data:image/png;base64,")
        .unwrap_or(value);
    decode_base64(payload)
}

fn decode_base64(input: &str) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(input.len() * 3 / 4);
    let mut chunk = [0u8; 4];
    let mut chunk_len = 0;
    for byte in input.bytes().filter(|byte| !byte.is_ascii_whitespace()) {
        let value = match byte {
            b'A'..=b'Z' => byte - b'A',
            b'a'..=b'z' => byte - b'a' + 26,
            b'0'..=b'9' => byte - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            b'=' => 64,
            _ => return None,
        };
        chunk[chunk_len] = value;
        chunk_len += 1;
        if chunk_len == 4 {
            if chunk[0] == 64 || chunk[1] == 64 {
                return None;
            }
            out.push((chunk[0] << 2) | (chunk[1] >> 4));
            if chunk[2] != 64 {
                out.push((chunk[1] << 4) | (chunk[2] >> 2));
            }
            if chunk[3] != 64 {
                out.push((chunk[2] << 6) | chunk[3]);
            }
            chunk_len = 0;
        }
    }
    if chunk_len == 0 {
        Some(out)
    } else {
        None
    }
}
