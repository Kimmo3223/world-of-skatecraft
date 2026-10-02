//! Derives `rig.json` and `board.json` from the skater GLB that the Skate 3 converter writes into
//! the player's own asset folder, on the player's machine, the first time the skate mode needs
//! them. Ported from the IW4L skate mode (chasmlol/2010-rust-rewrite-mashup,
//! `crates/assets/src/skate_board.rs`, Apache-2.0; this file stays Apache-2.0).

use std::path::Path;

use bevy::math::DMat4;
use serde::Serialize;
use serde_json::{Map, Value};

/// Board textures are box-filtered down to fit this on both sides.
const TEXTURE_LIMIT: u32 = 512;

/// Writes the two files unless both are already present.
pub(super) fn ensure(assets: &Path) -> Result<(), String> {
    if assets.join("rig.json").is_file() && assets.join("board.json").is_file() {
        return Ok(());
    }
    export(assets)
}

fn export(assets: &Path) -> Result<(), String> {
    let glb_path = assets.join("private").join("skater.glb");
    let bytes = std::fs::read(&glb_path)
        .map_err(|error| format!("cannot read {}: {error}", glb_path.display()))?;
    let glb = Glb::parse(&bytes)?;

    let skin = glb.at(&["skins", "0"])?;
    let joints = skin["joints"].as_array().ok_or("skin has no joints")?;
    let inverse_binds = glb.floats(index(&skin["inverseBindMatrices"])?)?;
    let names = joints
        .iter()
        .map(|joint| {
            let node = index(joint)?;
            glb.json["nodes"][node]["name"]
                .as_str()
                .map(str::to_owned)
                .ok_or_else(|| format!("joint node {node} has no name"))
        })
        .collect::<Result<Vec<_>, String>>()?;

    // The GLB carries Blender's bone basis; the retarget wants the native bind frame, the board
    // keeps the inverse binds as written.
    let basis_inverse = DMat4::from_cols_array(&[
        1., 0., 0., 0., 0., 0., -1., 0., 0., 1., 0., 0., 0., 0., 0., 1.,
    ])
    .inverse();
    let rig = names
        .iter()
        .zip(inverse_binds.as_chunks::<16>().0)
        .map(|(name, &inverse)| {
            let bind = DMat4::from_cols_array(&inverse.map(f64::from)).inverse() * basis_inverse;
            RigBone {
                name,
                bind: bind.to_cols_array(),
                inverse_bind: inverse,
            }
        })
        .collect::<Vec<_>>();

    let mut surfaces = Vec::new();
    let mut textures = Vec::new();
    let primitives = glb.at(&["meshes", "0", "primitives"])?;
    for primitive in primitives.as_array().ok_or("mesh has no primitives")? {
        let material = &glb.json["materials"][index(&primitive["material"])?];
        let material_name = material["name"].as_str().unwrap_or("");
        if !material_name.contains("Skate") {
            continue;
        }
        let texture = index(&material["pbrMetallicRoughness"]["baseColorTexture"]["index"])?;
        let image = index(&glb.json["textures"][texture]["source"])?;
        textures.push(glb.texture(image)?);

        let attributes = &primitive["attributes"];
        let positions = glb.floats(index(&attributes["POSITION"])?)?;
        let normals = glb.floats(index(&attributes["NORMAL"])?)?;
        let uvs = glb.floats(index(&attributes["TEXCOORD_0"])?)?;
        let bone_ids = glb.integers(index(&attributes["JOINTS_0"])?)?;
        let weights = glb.floats(index(&attributes["WEIGHTS_0"])?)?;
        let count = positions.len() / 3;
        if normals.len() != count * 3
            || uvs.len() != count * 2
            || bone_ids.len() != count * 4
            || weights.len() != count * 4
        {
            return Err(format!("{material_name}: vertex streams disagree"));
        }
        let vertices = (0..count)
            .map(|v| {
                let u = half::f16::from_f32(uvs[v * 2]).to_bits();
                let w = half::f16::from_f32(uvs[v * 2 + 1]).to_bits();
                BoardVertex {
                    position: [positions[v * 3], positions[v * 3 + 1], positions[v * 3 + 2]],
                    normal: [normals[v * 3], normals[v * 3 + 1], normals[v * 3 + 2]],
                    uv: (u32::from(u) << 16) | u32::from(w),
                    joints: std::array::from_fn(|i| bone_ids[v * 4 + i]),
                    weights: std::array::from_fn(|i| weights[v * 4 + i]),
                }
            })
            .collect();
        let indices = glb
            .integers(index(&primitive["indices"])?)?
            .as_chunks::<3>()
            .0
            .iter()
            .flat_map(|&[a, b, c]| [a, c, b])
            .collect();
        surfaces.push(BoardSurface {
            material: format!("iw4l_skate/{material_name}"),
            texture: textures.len() - 1,
            vertices,
            indices,
        });
    }
    if surfaces.is_empty() {
        return Err(format!("{} has no skateboard surfaces", glb_path.display()));
    }
    let board = Board {
        joints: names
            .iter()
            .map(|name| BoardJoint {
                target: name,
                origin: [0.; 3],
            })
            .collect(),
        surfaces,
        textures,
    };

    write_json(&assets.join("rig.json"), &rig)?;
    write_json(&assets.join("board.json"), &board)
}

fn write_json(path: &Path, value: &impl Serialize) -> Result<(), String> {
    let data = serde_json::to_vec(value).map_err(|error| error.to_string())?;
    let partial = path.with_extension("json.partial");
    std::fs::write(&partial, data)
        .and_then(|()| std::fs::rename(&partial, path))
        .map_err(|error| format!("cannot write {}: {error}", path.display()))
}

#[derive(Serialize)]
struct RigBone<'a> {
    name: &'a str,
    bind: [f64; 16],
    inverse_bind: [f32; 16],
}

#[derive(Serialize)]
struct Board<'a> {
    joints: Vec<BoardJoint<'a>>,
    surfaces: Vec<BoardSurface>,
    textures: Vec<BoardTexture>,
}

#[derive(Serialize)]
struct BoardJoint<'a> {
    target: &'a str,
    origin: [f32; 3],
}

#[derive(Serialize)]
struct BoardSurface {
    material: String,
    texture: usize,
    vertices: Vec<BoardVertex>,
    indices: Vec<u32>,
}

#[derive(Serialize)]
struct BoardVertex {
    position: [f32; 3],
    normal: [f32; 3],
    uv: u32,
    joints: [u32; 4],
    weights: [f32; 4],
}

#[derive(Serialize)]
struct BoardTexture {
    width: u32,
    height: u32,
    rgba: Vec<u8>,
}

fn index(value: &Value) -> Result<usize, String> {
    value
        .as_u64()
        .map(|v| v as usize)
        .ok_or_else(|| format!("expected an index, found {value}"))
}

struct Glb<'a> {
    json: Value,
    blob: &'a [u8],
}

impl<'a> Glb<'a> {
    fn parse(bytes: &'a [u8]) -> Result<Self, String> {
        let word = |at: usize| -> Result<usize, String> {
            bytes
                .get(at..at + 4)
                .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as usize)
                .ok_or_else(|| "truncated GLB".to_owned())
        };
        if bytes.get(..4) != Some(b"glTF") {
            return Err("skater.glb is not a GLB file".into());
        }
        let json_len = word(12)?;
        let json_bytes = bytes.get(20..20 + json_len).ok_or("truncated GLB JSON")?;
        let json = serde_json::from_slice(json_bytes).map_err(|error| error.to_string())?;
        let blob_len = word(20 + json_len)?;
        let blob_start = 28 + json_len;
        let blob = bytes
            .get(blob_start..blob_start + blob_len)
            .ok_or("truncated GLB binary chunk")?;
        Ok(Self { json, blob })
    }

    fn at(&self, path: &[&str]) -> Result<&Value, String> {
        let mut value = &self.json;
        for key in path {
            value = match key.parse::<usize>() {
                Ok(i) => &value[i],
                Err(_) => &value[*key],
            };
        }
        if value.is_null() {
            return Err(format!("skater.glb has no {}", path.join("/")));
        }
        Ok(value)
    }

    fn view(&self, view: usize) -> Result<(&'a [u8], Option<usize>), String> {
        let view = &self.json["bufferViews"][view];
        let offset = view["byteOffset"].as_u64().unwrap_or(0) as usize;
        let length = index(&view["byteLength"])?;
        let bytes = self
            .blob
            .get(offset..offset + length)
            .ok_or("buffer view outside the GLB")?;
        Ok((bytes, view["byteStride"].as_u64().map(|s| s as usize)))
    }

    /// Every component of an accessor, widened to `f64`.
    fn components(&self, accessor: usize) -> Result<Vec<f64>, String> {
        let accessor: &Map<String, Value> = self.json["accessors"][accessor]
            .as_object()
            .ok_or("missing accessor")?;
        let count = index(&accessor["count"])?;
        let width = match accessor["type"].as_str() {
            Some("SCALAR") => 1,
            Some("VEC2") => 2,
            Some("VEC3") => 3,
            Some("VEC4") => 4,
            Some("MAT4") => 16,
            other => return Err(format!("unsupported accessor type {other:?}")),
        };
        let (size, read): (usize, fn(&[u8]) -> f64) = match accessor["componentType"].as_u64() {
            Some(5121) => (1, |b| f64::from(b[0])),
            Some(5123) => (2, |b| f64::from(u16::from_le_bytes([b[0], b[1]]))),
            Some(5125) => (4, |b| {
                f64::from(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
            }),
            Some(5126) => (4, |b| {
                f64::from(f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
            }),
            other => return Err(format!("unsupported component type {other:?}")),
        };
        let (bytes, stride) = self.view(index(&accessor["bufferView"])?)?;
        let start = accessor
            .get("byteOffset")
            .and_then(Value::as_u64)
            .unwrap_or(0) as usize;
        let stride = stride.unwrap_or(size * width);
        let mut out = Vec::with_capacity(count * width);
        for element in 0..count {
            for component in 0..width {
                let at = start + element * stride + component * size;
                let field = bytes
                    .get(at..at + size)
                    .ok_or("accessor outside its view")?;
                out.push(read(field));
            }
        }
        Ok(out)
    }

    fn floats(&self, accessor: usize) -> Result<Vec<f32>, String> {
        Ok(self
            .components(accessor)?
            .into_iter()
            .map(|v| v as f32)
            .collect())
    }

    fn integers(&self, accessor: usize) -> Result<Vec<u32>, String> {
        Ok(self
            .components(accessor)?
            .into_iter()
            .map(|v| v as u32)
            .collect())
    }

    fn texture(&self, image: usize) -> Result<BoardTexture, String> {
        let view = index(&self.json["images"][image]["bufferView"])?;
        let (bytes, _) = self.view(view)?;
        let decoded = image::load_from_memory_with_format(bytes, image::ImageFormat::Png)
            .map_err(|error| format!("board texture {image}: {error}"))?
            .to_rgba8();
        let (width, height) = decoded.dimensions();
        Ok(shrink(width, height, decoded.into_raw()))
    }
}

/// Box-filters an image to fit within `TEXTURE_LIMIT` on both sides, keeping its aspect ratio.
/// Smaller images pass through.
fn shrink(width: u32, height: u32, rgba: Vec<u8>) -> BoardTexture {
    if width <= TEXTURE_LIMIT && height <= TEXTURE_LIMIT {
        return BoardTexture {
            width,
            height,
            rgba,
        };
    }
    let scale = (f64::from(TEXTURE_LIMIT) / f64::from(width))
        .min(f64::from(TEXTURE_LIMIT) / f64::from(height));
    let out_w = ((f64::from(width) * scale).round() as u32).max(1);
    let out_h = ((f64::from(height) * scale).round() as u32).max(1);
    let mut out = Vec::with_capacity((out_w * out_h * 4) as usize);
    for y in 0..out_h {
        let y0 = y * height / out_h;
        let y1 = ((y + 1) * height / out_h).max(y0 + 1);
        for x in 0..out_w {
            let x0 = x * width / out_w;
            let x1 = ((x + 1) * width / out_w).max(x0 + 1);
            let mut sum = [0u32; 4];
            for sy in y0..y1 {
                for sx in x0..x1 {
                    let at = ((sy * width + sx) * 4) as usize;
                    for (c, s) in sum.iter_mut().enumerate() {
                        *s += u32::from(rgba[at + c]);
                    }
                }
            }
            let n = (y1 - y0) * (x1 - x0);
            out.extend(sum.map(|s| ((s + n / 2) / n) as u8));
        }
    }
    BoardTexture {
        width: out_w,
        height: out_h,
        rgba: out,
    }
}

#[cfg(test)]
mod tests {
    /// Regenerates the files into a scratch copy and compares them with the IW4L launcher's,
    /// when `WOW_SKATE_ASSETS` points at a converted asset folder that has both.
    #[test]
    fn matches_the_iw4l_export() {
        let Some(root) = std::env::var_os("WOW_SKATE_ASSETS").map(std::path::PathBuf::from) else {
            return;
        };
        if !root.join("rig.json").is_file() {
            return;
        }
        let scratch = std::env::temp_dir().join(format!("skate-export-{}", std::process::id()));
        std::fs::create_dir_all(scratch.join("private")).unwrap();
        std::fs::copy(
            root.join("private/skater.glb"),
            scratch.join("private/skater.glb"),
        )
        .unwrap();
        super::export(&scratch).unwrap();
        for file in ["rig.json", "board.json"] {
            let ours: serde_json::Value =
                serde_json::from_slice(&std::fs::read(scratch.join(file)).unwrap()).unwrap();
            let theirs: serde_json::Value =
                serde_json::from_slice(&std::fs::read(root.join(file)).unwrap()).unwrap();
            assert_eq!(ours, theirs, "{file} differs");
        }
        let _ = std::fs::remove_dir_all(scratch);
    }
}
