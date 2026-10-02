//! The skateboard (`board.json`: deck, trucks and wheels from the converted Skate 3 skater),
//! skinned on the CPU to the skater's board joints each frame and drawn unlit under us.

use std::sync::OnceLock;

use bevy::asset::RenderAssetUsages;
use bevy::camera::visibility::NoFrustumCulling;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};

use super::{from_skate, rig::reference, SkatePose};

#[derive(serde::Deserialize)]
struct Joint {
    target: String,
}

#[derive(serde::Deserialize)]
struct Vertex {
    position: [f32; 3],
    normal: [f32; 3],
    /// Two `f16`s, u high.
    uv: u32,
    joints: [usize; 4],
    weights: [f32; 4],
}

#[derive(serde::Deserialize)]
struct Surface {
    texture: usize,
    vertices: Vec<Vertex>,
    indices: Vec<u32>,
}

#[derive(serde::Deserialize)]
struct Texture {
    width: u32,
    height: u32,
    rgba: Vec<u8>,
}

#[derive(serde::Deserialize)]
struct Model {
    joints: Vec<Joint>,
    surfaces: Vec<Surface>,
    textures: Vec<Texture>,
}

fn model() -> Option<&'static Model> {
    static MODEL: OnceLock<Option<Model>> = OnceLock::new();
    MODEL
        .get_or_init(|| {
            let root = super::assets_root()?;
            super::export::ensure(&root).map_err(|e| warn!("skate export: {e}")).ok()?;
            let data = std::fs::read(root.join("board.json")).ok()?;
            serde_json::from_slice(&data)
                .map_err(|e| warn!("skate board.json: {e}"))
                .ok()
        })
        .as_ref()
}

/// The spawned board: one entity a surface, positioned at the board each frame.
#[derive(Resource, Default)]
pub(super) struct Board {
    parts: Vec<(Entity, Handle<Mesh>)>,
}

fn spawn(
    model: &Model,
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    images: &mut Assets<Image>,
) -> Vec<(Entity, Handle<Mesh>)> {
    let textures: Vec<Handle<Image>> = model
        .textures
        .iter()
        .map(|t| {
            images.add(Image::new(
                Extent3d {
                    width: t.width,
                    height: t.height,
                    depth_or_array_layers: 1,
                },
                TextureDimension::D2,
                t.rgba.clone(),
                TextureFormat::Rgba8UnormSrgb,
                RenderAssetUsages::RENDER_WORLD,
            ))
        })
        .collect();
    model
        .surfaces
        .iter()
        .map(|s| {
            let uv = |p: u32| {
                [
                    half::f16::from_bits((p >> 16) as u16).to_f32(),
                    half::f16::from_bits(p as u16).to_f32(),
                ]
            };
            let mut mesh = Mesh::new(
                PrimitiveTopology::TriangleList,
                RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
            );
            mesh.insert_attribute(
                Mesh::ATTRIBUTE_POSITION,
                vec![[0.0f32; 3]; s.vertices.len()],
            );
            mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0f32, 1.0, 0.0]; s.vertices.len()]);
            mesh.insert_attribute(
                Mesh::ATTRIBUTE_UV_0,
                s.vertices.iter().map(|v| uv(v.uv)).collect::<Vec<_>>(),
            );
            mesh.insert_indices(Indices::U32(s.indices.clone()));
            let mesh = meshes.add(mesh);
            let material = materials.add(StandardMaterial {
                base_color_texture: textures.get(s.texture).cloned(),
                unlit: true,
                cull_mode: None,
                ..default()
            });
            let entity = commands
                .spawn((
                    Mesh3d(mesh.clone()),
                    MeshMaterial3d(material),
                    Transform::default(),
                    Visibility::Hidden,
                    NoFrustumCulling,
                ))
                .id();
            (entity, mesh)
        })
        .collect()
}

/// Skins the board to this frame's skater pose, or hides it off the board.
pub(super) fn update(
    pose: Res<SkatePose>,
    mut board: ResMut<Board>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
    mut parts: Query<(&mut Transform, &mut Visibility)>,
) {
    let Some(model) = model() else {
        return;
    };
    if !pose.active {
        for (entity, _) in &board.parts {
            if let Ok((_, mut v)) = parts.get_mut(*entity) {
                *v = Visibility::Hidden;
            }
        }
        return;
    }
    if board.parts.is_empty() {
        board.parts = spawn(model, &mut commands, &mut meshes, &mut materials, &mut images);
        return;
    }
    let Some(reference) = reference() else {
        return;
    };
    // The board's vertices are authored z up: this turns them y up before the joint's bind.
    let rb = Mat4::from_cols(Vec4::X, -Vec4::Z, Vec4::Y, Vec4::W);
    let matrices: Option<Vec<Mat4>> = model
        .joints
        .iter()
        .map(|j| {
            let posed = pose.bone(&j.target)?;
            let bind = reference.iter().find(|b| b.name == j.target)?;
            Some(posed * rb * Mat4::from_cols_array(&bind.inverse_bind))
        })
        .collect();
    let Some(matrices) = matrices else {
        static WARNED: std::sync::Once = std::sync::Once::new();
        WARNED.call_once(|| warn!("skate board: a board joint is missing from the pose"));
        return;
    };
    // Vertices relative to the deck, so no vertex is a far-off world coordinate.
    let anchor = pose
        .bone("SKATEBOARD_ROOT")
        .map_or(pose.origin, |m| from_skate(m.w_axis.truncate(), pose.origin));
    for (surface, (entity, handle)) in model.surfaces.iter().zip(&board.parts) {
        let Some(mesh) = meshes.get_mut(handle) else {
            continue;
        };
        let mut positions = Vec::with_capacity(surface.vertices.len());
        let mut normals = Vec::with_capacity(surface.vertices.len());
        for v in &surface.vertices {
            let mut p = Vec3::ZERO;
            let mut n = Vec3::ZERO;
            for i in 0..4 {
                let Some(m) = matrices.get(v.joints[i]) else {
                    continue;
                };
                p += m.transform_point3(Vec3::from_array(v.position)) * v.weights[i];
                n += m.transform_vector3(Vec3::from_array(v.normal)) * v.weights[i];
            }
            positions.push((from_skate(p, pose.origin) - anchor).to_array());
            normals.push(n.normalize_or(Vec3::Y).to_array());
        }
        if std::env::var_os("WOW_SKATE_DEBUG").is_some() {
            let lo = positions.iter().fold(Vec3::MAX, |a, p| a.min(Vec3::from_array(*p)));
            let hi = positions.iter().fold(Vec3::MIN, |a, p| a.max(Vec3::from_array(*p)));
            info!("skate board debug: surface {} spans {:?}..{:?} at {:?}", surface.texture, lo, hi, anchor);
        }
        mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
        mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
        if let Ok((mut t, mut vis)) = parts.get_mut(*entity) {
            t.translation = anchor;
            *vis = Visibility::Visible;
        }
    }
}
