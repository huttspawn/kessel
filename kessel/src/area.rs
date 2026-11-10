//! Area and room files: `/resources/world/areas/<area dir id>/area.dat` and
//! the room `.dat` files it lists.
//!
//! area.dat: a u32-length signature `AREA_DAT_BINARY_FORMAT_`; at 0x56 a u32
//! room count and that many u32-length room names; then a u32 asset count and
//! per asset a u64 id, a u32 length counting the NUL, and the asset path.
//!
//! Room .dat: a u32-length signature `ROOM_DAT_BINARY_FORMAT_`, then the u32
//! offset of the instance table; there a u32 count, and per instance: u32
//! `0xABCD1234`, u8, u64 instance id, u64 asset id, u8, u32 property count,
//! u32 property bytes, u8, then the properties: u8 type, u32 key, value
//! (0 u8; 1-4 u32 or f32; 5 u64; 6 vec3; 7 vec4; 8 u32 byte length +
//! UTF-16; 9 u32 length + blob).
//!
//! Positions are (x, height, z) relative to the parent instance, turned by
//! the parent's yaw; the combat log prints 10 x (x, z, height).

use anyhow::{bail, ensure, Context, Result};
use std::collections::HashMap;

const INSTANCE_MAGIC: u32 = 0xABCD_1234;
const ROOM_LIST_OFFSET: usize = 0x56;

const KEY_POSITION: u32 = 0x4f77_e269;
const KEY_ROTATION: u32 = 0x8c64_af1e;
const KEY_PARENT: u32 = 0x4086_5e7f;
const KEY_SCRIPT: u32 = 0xd84f_b395;
const KEY_SHAPE: u32 = 0x80af_3c1a;
const KEY_SIZE_X: u32 = 0xd9dd_f326;
const KEY_SIZE_HEIGHT: u32 = 0x16b4_f247;
const KEY_SIZE_Z: u32 = 0x1b20_5d23;

/// The parent chain is never deeper than this in a shipped area.
const MAX_PARENT_DEPTH: usize = 16;

/// An area.dat: its room names and its asset table (asset id to path).
pub struct AreaDat {
    pub rooms: Vec<String>,
    pub assets: HashMap<u64, String>,
}

/// One room instance, with the properties the world tables read.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Instance {
    pub id: u64,
    pub asset: u64,
    /// Parent instance id; 0 for none.
    pub parent: u64,
    /// (x, height, z), relative to the parent.
    pub position: [f32; 3],
    /// Euler degrees; [1] is the yaw.
    pub rotation: [f32; 3],
    pub script: Option<String>,
    pub shape: Option<u32>,
    /// Full sizes along local x, height and local z.
    pub size: [Option<f32>; 3],
}

struct Cursor<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Cursor<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8]> {
        let end = self.pos.checked_add(n).context("length overflow")?;
        let bytes = self.data.get(self.pos..end).context("truncated")?;
        self.pos = end;
        Ok(bytes)
    }
    fn u8(&mut self) -> Result<u8> {
        Ok(self.take(1)?[0])
    }
    fn u32(&mut self) -> Result<u32> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into()?))
    }
    fn u64(&mut self) -> Result<u64> {
        Ok(u64::from_le_bytes(self.take(8)?.try_into()?))
    }
    fn f32(&mut self) -> Result<f32> {
        Ok(f32::from_le_bytes(self.take(4)?.try_into()?))
    }
    fn vec3(&mut self) -> Result<[f32; 3]> {
        Ok([self.f32()?, self.f32()?, self.f32()?])
    }
    fn len_string(&mut self) -> Result<String> {
        let n = self.u32()? as usize;
        let bytes = self.take(n)?;
        Ok(String::from_utf8_lossy(bytes)
            .trim_end_matches('\0')
            .to_string())
    }
}

/// True when `data` begins with an area.dat's signature.
pub fn is_area_dat(data: &[u8]) -> bool {
    data.get(4..26) == Some(b"AREA_DAT_BINARY_FORMAT")
}

/// Read an area.dat's room names and asset table.
pub fn read_area(data: &[u8]) -> Result<AreaDat> {
    ensure!(is_area_dat(data), "not an area.dat");
    let mut c = Cursor { data, pos: 0 };
    c.pos = ROOM_LIST_OFFSET;
    let rooms = (0..c.u32()?)
        .map(|_| c.len_string())
        .collect::<Result<Vec<_>>>()?;
    let mut assets = HashMap::new();
    for _ in 0..c.u32()? {
        let id = c.u64()?;
        assets.insert(id, c.len_string()?);
    }
    Ok(AreaDat { rooms, assets })
}

/// Read every instance in a room .dat.
pub fn read_room(data: &[u8]) -> Result<Vec<Instance>> {
    let mut c = Cursor { data, pos: 0 };
    ensure!(
        c.len_string()?.starts_with("ROOM_DAT_BINARY_FORMAT"),
        "not a room .dat"
    );
    c.pos = c.u32()? as usize;
    let count = c.u32()?;
    let mut out = Vec::new();
    for _ in 0..count {
        ensure!(c.u32()? == INSTANCE_MAGIC, "instance magic");
        c.u8()?;
        let mut inst = Instance {
            id: c.u64()?,
            asset: c.u64()?,
            ..Instance::default()
        };
        c.u8()?;
        let props = c.u32()?;
        c.u32()?;
        c.u8()?;
        for _ in 0..props {
            let kind = c.u8()?;
            let key = c.u32()?;
            match kind {
                0 => {
                    c.u8()?;
                }
                1..=3 => {
                    let v = c.u32()?;
                    if key == KEY_SHAPE {
                        inst.shape = Some(v);
                    }
                }
                4 => {
                    let v = c.f32()?;
                    match key {
                        KEY_SIZE_X => inst.size[0] = Some(v),
                        KEY_SIZE_HEIGHT => inst.size[1] = Some(v),
                        KEY_SIZE_Z => inst.size[2] = Some(v),
                        _ => {}
                    }
                }
                5 => {
                    let v = c.u64()?;
                    if key == KEY_PARENT {
                        inst.parent = v;
                    }
                }
                6 => {
                    let v = c.vec3()?;
                    match key {
                        KEY_POSITION => inst.position = v,
                        KEY_ROTATION => inst.rotation = v,
                        _ => {}
                    }
                }
                7 => {
                    c.take(16)?;
                }
                8 => {
                    let n = c.u32()? as usize;
                    let units: Vec<u16> = c
                        .take(n)?
                        .as_chunks::<2>()
                        .0
                        .iter()
                        .map(|b| u16::from_le_bytes(*b))
                        .collect();
                    if key == KEY_SCRIPT {
                        let s = String::from_utf16_lossy(&units)
                            .trim_end_matches('\0')
                            .to_string();
                        inst.script = Some(s).filter(|s| !s.is_empty());
                    }
                }
                9 => {
                    let n = c.u32()? as usize;
                    c.take(n)?;
                }
                other => bail!("property type {other}"),
            }
        }
        out.push(inst);
    }
    Ok(out)
}

/// The spawner an asset path names: `\spn\a\b.ext` names `spn.a.b`, and an
/// encounter slot `\a:enc.<fqn>.ext` names slot `A` of encounter `enc.<fqn>`.
#[derive(Debug, PartialEq, Eq)]
pub enum SpawnerAsset {
    Spawner {
        fqn: String,
        ext: String,
    },
    Slot {
        encounter: String,
        slot: String,
        ext: String,
    },
}

impl SpawnerAsset {
    pub fn ext(&self) -> &str {
        match self {
            SpawnerAsset::Spawner { ext, .. } | SpawnerAsset::Slot { ext, .. } => ext,
        }
    }
}

/// What a room asset path names, when it is a spawner (`.spn_*`).
pub fn spawner_of_asset(path: &str) -> Option<SpawnerAsset> {
    let (stem, ext) = path.rsplit_once('.')?;
    if !ext.starts_with("spn_") {
        return None;
    }
    if let Some(rest) = stem.strip_prefix("\\spn\\") {
        return Some(SpawnerAsset::Spawner {
            fqn: format!("spn.{}", rest.replace('\\', ".")),
            ext: ext.to_string(),
        });
    }
    let (slot, enc) = stem.strip_prefix('\\')?.split_once(":enc.")?;
    Some(SpawnerAsset::Slot {
        encounter: format!("enc.{enc}"),
        slot: slot.to_uppercase(),
        ext: ext.to_string(),
    })
}

/// The spawner type an asset extension names (field 31d37823).
pub fn spawner_type(ext: &str) -> Option<&'static str> {
    match ext {
        "spn_c" => Some("creature"),
        "spn_p" => Some("placeable"),
        "spn_crf" => Some("crefac"),
        "spn_lst" => Some("list"),
        _ => None,
    }
}

/// An instance's world position (x, height, z) and yaw in degrees, composed
/// through its parent chain: each position is relative to its parent, turned
/// by the parent's yaw. `None` when a parent is missing or the chain is too deep.
pub fn world_of(id: u64, by_id: &HashMap<u64, &Instance>) -> Option<([f32; 3], f32)> {
    let inst = by_id.get(&id)?;
    let mut pos = inst.position;
    let mut yaw = inst.rotation[1];
    let mut cur = inst.parent;
    let mut depth = 0;
    while cur != 0 {
        let p = by_id.get(&cur)?;
        let [x, z] = turn(pos[0], pos[2], p.rotation[1]);
        pos = [p.position[0] + x, p.position[1] + pos[1], p.position[2] + z];
        yaw += p.rotation[1];
        cur = p.parent;
        depth += 1;
        if depth > MAX_PARENT_DEPTH {
            return None;
        }
    }
    Some((pos, yaw))
}

/// Turn (x, z) by `yaw` degrees about the height axis.
pub fn turn(x: f32, z: f32, yaw: f32) -> [f32; 2] {
    let (s, c) = yaw.to_radians().sin_cos();
    [x * c + z * s, -x * s + z * c]
}

/// World (x, height, z) to the combat log's frame: 10 x (x, z, height).
pub fn to_log(world: [f32; 3]) -> [f64; 3] {
    [
        f64::from(world[0]) * 10.0,
        f64::from(world[2]) * 10.0,
        f64::from(world[1]) * 10.0,
    ]
}

/// The log-frame axis-aligned box enclosing a box volume of full `size`
/// (x, height, z) centred on `centre`, turned by `yaw`: (min, max).
pub fn log_bounds(centre: [f32; 3], size: [f32; 3], yaw: f32) -> ([f64; 3], [f64; 3]) {
    let mut lo = [f64::MAX; 3];
    let mut hi = [f64::MIN; 3];
    for corner in 0..8 {
        let half = |bit: i32, s: f32| if corner & bit == 0 { -s / 2.0 } else { s / 2.0 };
        let [x, z] = turn(half(1, size[0]), half(4, size[2]), yaw);
        let p = to_log([centre[0] + x, centre[1] + half(2, size[1]), centre[2] + z]);
        for i in 0..3 {
            lo[i] = lo[i].min(p[i]);
            hi[i] = hi[i].max(p[i]);
        }
    }
    (lo, hi)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn len_str(out: &mut Vec<u8>, s: &str) {
        out.extend_from_slice(&u32::try_from(s.len() + 1).unwrap().to_le_bytes());
        out.extend_from_slice(s.as_bytes());
        out.push(0);
    }

    #[test]
    fn read_area_reads_rooms_and_assets() {
        let mut d = Vec::new();
        len_str(&mut d, "AREA_DAT_BINARY_FORMAT_");
        d.resize(ROOM_LIST_OFFSET, 0);
        d.extend_from_slice(&2u32.to_le_bytes());
        len_str(&mut d, "default_01");
        len_str(&mut d, "hutta_west");
        d.extend_from_slice(&1u32.to_le_bytes());
        d.extend_from_slice(&0x4000_0000_0000_0007u64.to_le_bytes());
        len_str(&mut d, "\\spn\\codex\\species\\hutt.spn_p");
        let a = read_area(&d).unwrap();
        assert_eq!(a.rooms, ["default_01", "hutta_west"]);
        assert_eq!(
            a.assets.get(&0x4000_0000_0000_0007).map(String::as_str),
            Some("\\spn\\codex\\species\\hutt.spn_p")
        );
        assert!(read_area(b"\x04\0\0\0nope").is_err());
    }

    fn prop(out: &mut Vec<u8>, kind: u8, key: u32, value: &[u8]) {
        out.push(kind);
        out.extend_from_slice(&key.to_le_bytes());
        out.extend_from_slice(value);
    }

    #[test]
    fn read_room_reads_instance_properties() {
        let mut props = Vec::new();
        let pos: Vec<u8> = [1.0f32, 2.0, 3.0]
            .iter()
            .flat_map(|f| f.to_le_bytes())
            .collect();
        prop(&mut props, 6, KEY_POSITION, &pos);
        prop(&mut props, 5, KEY_PARENT, &9u64.to_le_bytes());
        prop(&mut props, 4, KEY_SIZE_X, &4.5f32.to_le_bytes());
        prop(&mut props, 3, KEY_SHAPE, &6u32.to_le_bytes());
        prop(&mut props, 7, 0x1234, &[0; 16]);
        let script: Vec<u8> = "hyd.a\0"
            .encode_utf16()
            .flat_map(u16::to_le_bytes)
            .collect();
        let mut s = u32::try_from(script.len()).unwrap().to_le_bytes().to_vec();
        s.extend_from_slice(&script);
        prop(&mut props, 8, KEY_SCRIPT, &s);
        prop(&mut props, 9, 0x5678, &[2, 0, 0, 0, 0xaa, 0xbb]);

        let mut d = Vec::new();
        len_str(&mut d, "ROOM_DAT_BINARY_FORMAT_");
        let table = d.len() + 4 + 3;
        d.extend_from_slice(&u32::try_from(table).unwrap().to_le_bytes());
        d.extend_from_slice(&[0; 3]);
        d.extend_from_slice(&1u32.to_le_bytes());
        d.extend_from_slice(&INSTANCE_MAGIC.to_le_bytes());
        d.push(0);
        d.extend_from_slice(&7u64.to_le_bytes());
        d.extend_from_slice(&8u64.to_le_bytes());
        d.push(0);
        d.extend_from_slice(&7u32.to_le_bytes());
        d.extend_from_slice(&u32::try_from(props.len()).unwrap().to_le_bytes());
        d.push(0);
        d.extend_from_slice(&props);

        let room = read_room(&d).unwrap();
        assert_eq!(
            room,
            [Instance {
                id: 7,
                asset: 8,
                parent: 9,
                position: [1.0, 2.0, 3.0],
                rotation: [0.0; 3],
                script: Some("hyd.a".into()),
                shape: Some(6),
                size: [Some(4.5), None, None],
            }]
        );
    }

    #[test]
    fn spawner_paths_name_spawners_and_slots() {
        assert_eq!(
            spawner_of_asset("\\spn\\codex\\species\\hutt.spn_p"),
            Some(SpawnerAsset::Spawner {
                fqn: "spn.codex.species.hutt".into(),
                ext: "spn_p".into()
            })
        );
        assert_eq!(
            spawner_of_asset("\\b:enc.location.hutta.enc_nohn.spn_c"),
            Some(SpawnerAsset::Slot {
                encounter: "enc.location.hutta.enc_nohn".into(),
                slot: "B".into(),
                ext: "spn_c".into()
            })
        );
        assert_eq!(spawner_of_asset("\\engine\\trigger.trg"), None);
        assert_eq!(spawner_type("spn_c"), Some("creature"));
        assert_eq!(spawner_type("spn_p"), Some("placeable"));
        assert_eq!(spawner_type("spn_crf"), Some("crefac"));
        assert_eq!(spawner_type("spn_lst"), Some("list"));
    }

    #[test]
    fn slot_is_placed_from_its_parent_turned_by_yaw() {
        let parent = Instance {
            id: 1,
            position: [10.0, 5.0, 20.0],
            rotation: [0.0, 90.0, 0.0],
            ..Instance::default()
        };
        let slot = Instance {
            id: 2,
            parent: 1,
            position: [1.0, 0.5, 0.0],
            rotation: [0.0, 10.0, 0.0],
            ..Instance::default()
        };
        let by_id: HashMap<u64, &Instance> = [(1, &parent), (2, &slot)].into();
        let (pos, yaw) = world_of(2, &by_id).unwrap();
        assert!((pos[0] - 10.0).abs() < 1e-5);
        assert!((pos[1] - 5.5).abs() < 1e-5);
        assert!((pos[2] - 19.0).abs() < 1e-5);
        assert!((yaw - 100.0).abs() < 1e-5);
        assert_eq!(world_of(3, &by_id), None);
    }

    #[test]
    fn log_frame_is_ten_times_x_z_height() {
        assert_eq!(to_log([1.5, 2.0, -3.0]), [15.0, -30.0, 20.0]);
        let (lo, hi) = log_bounds([0.0, 0.0, 0.0], [2.0, 4.0, 6.0], 90.0);
        let close = |a: [f64; 3], b: [f64; 3]| a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-4);
        assert!(close(lo, [-30.0, -10.0, -20.0]), "{lo:?}");
        assert!(close(hi, [30.0, 10.0, 20.0]), "{hi:?}");
    }

    #[test]
    fn is_area_dat_reads_the_signature() {
        let mut d = Vec::new();
        len_str(&mut d, "AREA_DAT_BINARY_FORMAT_");
        assert!(is_area_dat(&d));
        let mut room = Vec::new();
        len_str(&mut room, "ROOM_DAT_BINARY_FORMAT_");
        assert!(!is_area_dat(&room));
        assert!(!is_area_dat(&[0xff, 0xff, 0xff, 0xff]));
    }
}
