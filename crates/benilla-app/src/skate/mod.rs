//! Skate 3 mode: `J` drops our character onto a board driven by the Skate 3 engine (`skate-host`,
//! as in the IW4L skate mode). The engine runs on a worker thread over the world's collision
//! gathered around us; while it rides, its pose is our position, facing and camera (read by
//! `player::controller`), its skeleton is retargeted onto our character's ([`rig`]) and the board
//! is drawn under it ([`board`]). The controller is an Xbox-style pad, read by the engine itself.

mod board;
mod rails;
mod rig;
mod sound;

use std::path::{Path, PathBuf};
use std::sync::{mpsc, Mutex};

use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use benilla_world::collision::WorldCollision;
use benilla_world::schedule::WorldStage;
use skate_host::bridge::{ControllerTransport, InputFrame, PreparedCollision, Session};

/// The engine's unit is the meter; ours is the yard.
const METERS_PER_YARD: f32 = 0.9144;
/// Half extents (yards) of the collision box gathered around the skater.
const GATHER_HALF: Vec3 = Vec3::new(70.0, 45.0, 70.0);
/// How far (yards) the skater rolls from the box centre before it is rebuilt around them.
const RECENTRE: f32 = 30.0;
/// Seconds between rebuilds while riding, so terrain streamed in since becomes solid.
const REFRESH_SECS: f32 = 5.0;
/// Below this speed (yd/s) the board counts as standing still on the wire.
const MOVING_SPEED: f32 = 0.5;

type Tri = [[f32; 3]; 3];

/// Our world (Bevy yards) to the engine's: meters from the session's `origin`, same axes (both
/// are right-handed, +Y up), which keeps the engine's floats small anywhere on a continent.
pub(crate) fn to_skate(p: Vec3, origin: Vec3) -> Vec3 {
    (p - origin) * METERS_PER_YARD
}

/// Inverse of [`to_skate`].
pub(crate) fn from_skate(p: Vec3, origin: Vec3) -> Vec3 {
    p / METERS_PER_YARD + origin
}

/// The skate engine's assets, converted from Skate 3 (`board.json`, `rig.json`, the banks):
/// `WOW_SKATE_ASSETS`, else a `skate-data/assets` beside or above the working directory.
pub(crate) fn assets_root() -> Option<PathBuf> {
    static ROOT: std::sync::OnceLock<Option<PathBuf>> = std::sync::OnceLock::new();
    ROOT.get_or_init(|| {
        std::env::var_os("WOW_SKATE_ASSETS")
            .map(PathBuf::from)
            .or_else(|| {
                ["skate-data/assets", "../skate-data/assets"]
                    .into_iter()
                    .map(PathBuf::from)
                    .find(|p| p.join("rig.json").is_file())
            })
    })
    .clone()
}

/// What the controller reads while we ride: it drives the body and the wire from this instead of
/// the mover.
#[derive(Resource, Default)]
pub(crate) struct SkateDrive {
    pub(crate) active: bool,
    /// Feet, Bevy world.
    pub(crate) pos: Vec3,
    /// Facing (Bevy yaw): the direction of travel, else the board's heading.
    pub(crate) yaw: f32,
    /// Horizontal speed, yd/s.
    pub(crate) speed: f32,
    /// The engine's own follow camera, Bevy world.
    pub(crate) camera: Option<Transform>,
    /// Ride with the engine's camera rather than our follow camera (`K` toggles).
    pub(crate) engine_camera: bool,
}

impl SkateDrive {
    pub(crate) fn moving(&self) -> bool {
        self.speed > MOVING_SPEED
    }
}

/// The skater's latest solved skeleton, for the retarget and the board: bones relative to the
/// skater's root, the root in engine space (meters from `origin`).
#[derive(Resource, Default)]
pub(crate) struct SkatePose {
    pub(crate) active: bool,
    pub(crate) origin: Vec3,
    pub(crate) root: Mat4,
    pub(crate) bones: Vec<Mat4>,
    pub(crate) names: Vec<String>,
}

impl SkatePose {
    /// A bone in engine space.
    pub(crate) fn bone(&self, name: &str) -> Option<Mat4> {
        let i = self.names.iter().position(|n| n == name)?;
        Some(self.root * *self.bones.get(i)?)
    }
}

enum Job {
    /// Start riding at `spawn` (engine space) on fresh collision; a new session the first time.
    Activate {
        epoch: u64,
        triangles: Vec<Tri>,
        spawn: Vec3,
        heading: f32,
        aspect: f32,
    },
    /// Collision gathered around the skater, built off the simulation and swapped in.
    Rebuild(u64, Vec<Tri>),
    Step {
        epoch: u64,
        dt: f32,
        input: InputFrame,
        aspect: f32,
    },
    Suspend,
}

enum Reply {
    Activated(u64, Frame),
    Pose(u64, Frame),
    Error(String),
}

/// One engine pose, out of the engine's own math types (its Bevy is not ours).
struct Frame {
    root: Mat4,
    bones: Vec<Mat4>,
    names: Vec<String>,
    camera: Option<(Vec3, Mat3)>,
    velocity: Vec3,
    tick: u64,
    state: String,
}

fn frame(p: skate_host::bridge::Pose) -> Frame {
    let m = |c: [f32; 16]| Mat4::from_cols_array(&c);
    Frame {
        root: m(p.root.to_cols_array()),
        bones: p.bones.iter().map(|b| m(b.to_cols_array())).collect(),
        names: p.names,
        camera: p.camera.map(|(position, basis, _)| {
            (
                Vec3::from_array(position.to_array()),
                Mat3::from_cols_array(&basis.to_cols_array()),
            )
        }),
        velocity: Vec3::from_array(p.velocity.to_array()),
        tick: p.tick,
        state: p.state,
    }
}

#[derive(Resource, Default)]
struct Host {
    send: Option<mpsc::Sender<Job>>,
    receive: Option<Mutex<mpsc::Receiver<Reply>>>,
    transport: Mutex<ControllerTransport>,
    epoch: u64,
    /// `J` was pressed and the worker is placing the skater.
    activating: bool,
    /// The session's engine-space origin, Bevy world.
    origin: Vec3,
    /// Where the collision was last gathered around, and when.
    centre: Vec3,
    gathered_at: f32,
    logged_tick: u64,
    /// The `WOW_SKATE_AUTOSTART` hook: when we became free to ride, and whether it has fired.
    ready_since: Option<f32>,
    autostarted: bool,
    /// Whether the last poll found a pad, so connects and losses are logged once.
    pad_seen: bool,
}

fn autostart_after() -> Option<f32> {
    static AFTER: std::sync::OnceLock<Option<f32>> = std::sync::OnceLock::new();
    *AFTER.get_or_init(|| std::env::var("WOW_SKATE_AUTOSTART").ok()?.parse().ok())
}

/// Grind rails over engine-space triangles: [`rails::find`] runs in inches, z up.
fn find_rails(tris: &[Tri]) -> Vec<Vec<[f32; 3]>> {
    const INCH: f32 = 0.0254;
    let z_up: Vec<[Vec3; 3]> = tris
        .iter()
        .map(|t| t.map(|p| Vec3::new(p[0], -p[2], p[1]) / INCH))
        .collect();
    let (found, census) = rails::find(&z_up);
    info!(
        "skate rails: {} walkable edges, {} lips, {} runs, {} rails",
        census.candidates, census.lips, census.runs, census.rails
    );
    found
        .into_iter()
        .map(|rail| {
            rail.into_iter()
                .map(|v| [v.x * INCH, v.z * INCH, -v.y * INCH])
                .collect()
        })
        .collect()
}

fn spawn_worker(root: PathBuf) -> std::io::Result<(mpsc::Sender<Job>, mpsc::Receiver<Reply>)> {
    let (send, jobs) = mpsc::channel();
    let (publish, replies) = mpsc::channel();
    std::thread::Builder::new()
        .name("skate".into())
        .stack_size(32 * 1024 * 1024)
        .spawn(move || {
            if let Err(e) = run_worker(&root, jobs, &publish) {
                let _ = publish.send(Reply::Error(e));
            }
        })?;
    Ok((send, replies))
}

/// The worker: one retained session, reactivated at each `J`.
fn run_worker(
    root: &Path,
    jobs: mpsc::Receiver<Job>,
    publish: &mpsc::Sender<Reply>,
) -> Result<(), String> {
    let mut session: Option<Session> = None;
    let (built_send, built) = mpsc::channel::<(u64, Result<PreparedCollision, String>)>();
    let mut builds: Option<mpsc::Sender<(u64, Vec<Tri>)>> = None;
    let mut epoch = 0;
    let mut accumulated = 0.0;
    while let Ok(job) = jobs.recv() {
        match job {
            Job::Activate {
                epoch: e,
                triangles,
                spawn,
                heading,
                aspect,
            } => {
                epoch = e;
                accumulated = 0.0;
                let start = std::time::Instant::now();
                let rails = find_rails(&triangles);
                let s = match session.as_mut() {
                    Some(s) => {
                        let prepared = s.collision_builder().build(triangles, rails)?;
                        s.install_collision(prepared)?;
                        s
                    }
                    None => {
                        let s = Session::new(root, triangles, rails, spawn.to_array(), heading)?;
                        let builder = s.collision_builder();
                        let (tx, rx) = mpsc::channel::<(u64, Vec<Tri>)>();
                        let built_send = built_send.clone();
                        std::thread::Builder::new()
                            .name("skate-collision".into())
                            .stack_size(32 * 1024 * 1024)
                            .spawn(move || {
                                while let Ok(mut job) = rx.recv() {
                                    while let Ok(newer) = rx.try_recv() {
                                        job = newer;
                                    }
                                    let (e, tris) = job;
                                    let rails = find_rails(&tris);
                                    if built_send.send((e, builder.build(tris, rails))).is_err() {
                                        break;
                                    }
                                }
                            })
                            .map_err(|e| e.to_string())?;
                        builds = Some(tx);
                        session.insert(s)
                    }
                };
                s.set_aspect_ratio(aspect);
                let p = s.activate(spawn.to_array(), heading)?;
                info!("skate: on the board in {} ms", start.elapsed().as_millis());
                if publish.send(Reply::Activated(epoch, frame(p))).is_err() {
                    break;
                }
            }
            Job::Rebuild(e, tris) => {
                if let Some(tx) = &builds {
                    let _ = tx.send((e, tris));
                }
            }
            Job::Suspend => {
                accumulated = 0.0;
                if let Some(s) = session.as_mut() {
                    s.suspend_input();
                }
            }
            Job::Step {
                epoch: e,
                dt,
                input,
                aspect,
            } => {
                let Some(s) = session.as_mut().filter(|_| e == epoch) else {
                    continue;
                };
                while let Ok((built_epoch, prepared)) = built.try_recv() {
                    match prepared {
                        Ok(p) if built_epoch == epoch => s.install_collision(p)?,
                        Ok(_) => {}
                        Err(e) => warn!("skate collision: {e}"),
                    }
                }
                s.set_aspect_ratio(aspect);
                s.collect(input, dt);
                accumulated = (accumulated + dt).min(0.15);
                let mut advanced = false;
                // The engine's camera can change the simulation period.
                while accumulated >= s.period() {
                    accumulated -= s.period();
                    s.advance()?;
                    advanced = true;
                }
                if advanced {
                    let p = s.pose();
                    if !p.root.is_finite() || p.bones.iter().any(|b| !b.is_finite()) {
                        return Err("the engine published a non-finite pose".into());
                    }
                    if publish.send(Reply::Pose(epoch, frame(p))).is_err() {
                        break;
                    }
                }
            }
        }
    }
    Ok(())
}

/// Every collision triangle in the box around `centre`, engine space; zero-area faces are left
/// out, as their NaN normals poison the board's physics.
fn gather(collide: &WorldCollision, centre: Vec3, origin: Vec3) -> Vec<Tri> {
    collide
        .faces_near_body(centre, GATHER_HALF, usize::MAX)
        .into_iter()
        .filter_map(|face| {
            let t = face.verts().map(|v| to_skate(v, origin));
            let solid = t.iter().all(|v| v.is_finite())
                && (t[1] - t[0]).cross(t[2] - t[0]).length_squared() > 1e-6;
            solid.then(|| t.map(|v| v.to_array()))
        })
        .collect()
}

fn stop(host: &mut Host, drive: &mut SkateDrive, pose: &mut SkatePose) {
    host.epoch = host.epoch.wrapping_add(1);
    host.activating = false;
    if let Some(send) = &host.send {
        let _ = send.send(Job::Suspend);
    }
    if drive.active {
        info!("skate: off the board");
    }
    drive.active = false;
    drive.camera = None;
    pose.active = false;
}

fn present(frame: Frame, origin: Vec3, drive: &mut SkateDrive, pose: &mut SkatePose) {
    drive.pos = from_skate(frame.root.w_axis.truncate(), origin);
    let flat = Vec3::new(frame.velocity.x, 0.0, frame.velocity.z);
    drive.speed = flat.length() / METERS_PER_YARD;
    let heading = if drive.moving() {
        flat
    } else {
        let z = frame.root.z_axis.truncate();
        Vec3::new(z.x, 0.0, z.z)
    };
    if heading.length_squared() > 1e-6 {
        // Bevy yaw `θ` faces `(−sin θ, 0, −cos θ)`.
        drive.yaw = f32::atan2(-heading.x, -heading.z);
    }
    // The engine frames a skater; a WoW character reads small at that range, so the camera is
    // pulled back along its line to the hips.
    let hips = frame
        .names
        .iter()
        .position(|n| n == "HIPS")
        .and_then(|i| frame.bones.get(i))
        .map(|b| frame.root.transform_point3(b.w_axis.truncate()));
    drive.camera = frame.camera.and_then(|(position, basis)| {
        let position = hips.map_or(position, |h| h + (position - h) * camera_pull());
        let at = from_skate(position, origin);
        let look = basis.z_axis.try_normalize()?;
        let up = basis.y_axis.try_normalize()?;
        Some(Transform::from_translation(at).looking_to(look, up))
    });
    pose.origin = origin;
    // The engine leaves the root's last row unset (its teleport writes `w = 0`).
    pose.root = Mat4::from_cols(
        frame.root.x_axis,
        frame.root.y_axis,
        frame.root.z_axis,
        frame.root.w_axis.truncate().extend(1.0),
    );
    pose.bones = frame.bones;
    pose.names = frame.names;
}

/// How much farther than the engine's own framing the skate camera sits: `WOW_SKATE_CAM_PULL`,
/// default 2.
fn camera_pull() -> f32 {
    static PULL: std::sync::OnceLock<f32> = std::sync::OnceLock::new();
    *PULL.get_or_init(|| {
        std::env::var("WOW_SKATE_CAM_PULL")
            .ok()
            .and_then(|v| v.parse().ok())
            .filter(|v: &f32| *v > 0.0)
            .unwrap_or(2.0)
    })
}

/// Decodes the animation banks while the world loads, so the first `J` only places the skater.
fn preload() {
    let Some(root) = assets_root() else {
        info!("skate: no skate-data/assets (set WOW_SKATE_ASSETS); J is off");
        return;
    };
    let _ = std::thread::Builder::new()
        .name("skate-preload".into())
        .stack_size(32 * 1024 * 1024)
        .spawn(move || {
            let start = std::time::Instant::now();
            match Session::preload(&root) {
                Ok(()) => info!(
                    "skate: animation banks preloaded in {} ms",
                    start.elapsed().as_millis()
                ),
                Err(e) => warn!("skate preload: {e}"),
            }
        });
}

#[allow(clippy::too_many_arguments)]
fn update(
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    ui: Res<crate::ui_script::UiKeyboardCapture>,
    player: Res<crate::player::Player>,
    collide: WorldCollision,
    windows: Query<&Window, With<PrimaryWindow>>,
    body: Query<Entity, With<crate::net::Embodied>>,
    mut sheath: MessageWriter<crate::creature_anim::SheathRequest>,
    mut host: ResMut<Host>,
    mut drive: ResMut<SkateDrive>,
    mut pose: ResMut<SkatePose>,
    mut audio: ResMut<sound::SkateAudio>,
) {
    let aspect = windows
        .single()
        .map(|w| w.width() / w.height().max(1.0))
        .unwrap_or(16.0 / 9.0);
    let riding = drive.active || host.activating;
    if riding && !player.may_skate() {
        stop(&mut host, &mut drive, &mut pose);
    }
    if keys.just_pressed(KeyCode::KeyK) && !ui.typing && drive.active {
        drive.engine_camera = !drive.engine_camera;
    }
    // Test hook: `WOW_SKATE_AUTOSTART=<seconds>` presses `J` once, that long after we may ride.
    let autostart = autostart_after().is_some_and(|after| {
        if !player.may_skate() || host.autostarted {
            host.ready_since = None;
            return false;
        }
        let since = *host.ready_since.get_or_insert(time.elapsed_secs());
        let due = time.elapsed_secs() - since >= after;
        host.autostarted |= due;
        due
    });
    if (keys.just_pressed(KeyCode::KeyJ) && !ui.typing) || autostart {
        if drive.active || host.activating {
            stop(&mut host, &mut drive, &mut pose);
        } else if player.may_skate() {
            start(&time, &collide, &player, aspect, &mut host);
        }
    }

    let mut replies = Vec::new();
    if let Some(receive) = &host.receive {
        let receive = receive.lock().unwrap();
        loop {
            match receive.try_recv() {
                Ok(reply) => replies.push(reply),
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => {
                    replies.push(Reply::Error("the skate worker stopped".into()));
                    break;
                }
            }
        }
    }
    for reply in replies {
        match reply {
            Reply::Activated(epoch, f) if epoch == host.epoch && host.activating => {
                host.activating = false;
                drive.active = true;
                // `WOW_SKATE_CAM=engine` starts on the engine's camera.
                drive.engine_camera |= std::env::var("WOW_SKATE_CAM").is_ok_and(|v| v == "engine");
                // Weapons stow on the back: in hand they follow the skater's wrists, whose roll
                // swings a sword or shield about.
                if let Ok(entity) = body.single() {
                    sheath.write(crate::creature_anim::SheathRequest {
                        entity,
                        state: 0,
                        ceremony: false,
                    });
                }
                pose.active = true;
                audio.reset();
                audio.observe(&f.state, f.velocity);
                present(f, host.origin, &mut drive, &mut pose);
            }
            Reply::Pose(epoch, f) if epoch == host.epoch && drive.active => {
                if f.tick / 120 != host.logged_tick / 120 {
                    info!(
                        "skate tick={} speed={:.2} yd/s state={}",
                        f.tick,
                        f.velocity.length() / METERS_PER_YARD,
                        f.state
                    );
                    host.logged_tick = f.tick;
                    if std::env::var_os("WOW_SKATE_DEBUG").is_some() {
                        let at = |n: &str| {
                            f.names
                                .iter()
                                .position(|x| x == n)
                                .map(|i| f.bones[i].w_axis.truncate())
                        };
                        info!(
                            "skate debug: root {:?} hips {:?} lfoot {:?} rfoot {:?} board {:?} tf {:?} tb {:?} wfl {:?} wbr {:?} ltoe_rep {:?}",
                            f.root.w_axis.truncate(),
                            at("HIPS"),
                            at("LEFTFOOT"),
                            at("RIGHTFOOT"),
                            at("SKATEBOARD_ROOT"),
                            at("TRUCK_FRONT"),
                            at("TRUCK_BACK"),
                            at("LEFT_WHEELFRONT"),
                            at("RIGHT_WHEELBACK"),
                            at("LEFTTOEBASE_REPARENTED"),
                        );
                    }
                }
                audio.observe(&f.state, f.velocity);
                present(f, host.origin, &mut drive, &mut pose);
            }
            Reply::Error(e) => {
                warn!("skate stopped: {e}");
                stop(&mut host, &mut drive, &mut pose);
                host.send = None;
                host.receive = None;
                return;
            }
            _ => {}
        }
    }
    if !drive.active {
        return;
    }

    let now = time.elapsed_secs();
    let rolled = drive.pos - host.centre;
    if Vec3::new(rolled.x, 0.0, rolled.z).length() > RECENTRE
        || rolled.y.abs() > GATHER_HALF.y * 0.5
        || now - host.gathered_at > REFRESH_SECS
    {
        let triangles = gather(&collide, drive.pos, host.origin);
        host.centre = drive.pos;
        host.gathered_at = now;
        if let Some(send) = &host.send {
            let _ = send.send(Job::Rebuild(host.epoch, triangles));
        }
    }
    let input = host.transport.lock().unwrap().poll();
    let pad = input.controller().is_some();
    if pad != host.pad_seen {
        host.pad_seen = pad;
        match pad {
            true => info!("skate: controller connected"),
            false => warn!("skate: no controller found; the board needs an Xbox-style pad"),
        }
    }
    if let Some(send) = &host.send {
        let job = Job::Step {
            epoch: host.epoch,
            dt: time.delta_secs().min(0.1),
            input,
            aspect,
        };
        if send.send(job).is_err() {
            stop(&mut host, &mut drive, &mut pose);
        }
    }
}

fn start(
    time: &Time,
    collide: &WorldCollision,
    player: &crate::player::Player,
    aspect: f32,
    host: &mut Host,
) {
    if host.send.is_none() {
        let Some(root) = assets_root() else {
            warn!("skate: no skate-data/assets found (set WOW_SKATE_ASSETS)");
            return;
        };
        match spawn_worker(root) {
            Ok((send, receive)) => {
                host.send = Some(send);
                host.receive = Some(Mutex::new(receive));
            }
            Err(e) => {
                warn!("skate worker: {e}");
                return;
            }
        }
    }
    host.epoch = host.epoch.wrapping_add(1);
    host.activating = true;
    host.origin = player.pos;
    host.centre = player.pos;
    host.gathered_at = time.elapsed_secs();
    let triangles = gather(collide, player.pos, host.origin);
    info!(
        "skate: {} collision triangles around {:?}",
        triangles.len(),
        player.pos
    );
    // The engine's heading `h` faces `(sin h, 0, cos h)`; our yaw `θ` faces `(−sin θ, 0, −cos θ)`.
    let heading = player.face_yaw() + std::f32::consts::PI;
    if let Some(send) = &host.send {
        let _ = send.send(Job::Activate {
            epoch: host.epoch,
            triangles,
            spawn: to_skate(player.pos + Vec3::Y * 0.1, host.origin),
            heading,
            aspect,
        });
    }
}

pub(crate) fn plugin(app: &mut App) {
    app.init_resource::<Host>()
        .init_resource::<SkateDrive>()
        .init_resource::<SkatePose>()
        .init_resource::<board::Board>()
        .init_resource::<sound::SkateAudio>()
        .add_systems(Startup, preload)
        .add_systems(
            Update,
            (update, board::update, sound::update)
                .chain()
                .in_set(WorldStage::Input)
                .before(crate::player::PlayerControlSet)
                .in_set(crate::char_select::InWorldGated),
        )
        .add_systems(
            PostUpdate,
            rig::retarget.in_set(benilla_world::rig_anim::PosePost),
        );
}
