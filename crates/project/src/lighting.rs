//! The 3D scene of a sequence: its coloured, placeable light objects (instead of one global
//! light angle) and the camera it renders through. An empty light list and no camera mean the
//! classic look (one light per object, a fixed frame-height focal), so old projects render
//! unchanged.

use serde::{Deserialize, Serialize};

/// What a light is: an aimed beam from infinity, a bulb in space, or a cone from a point.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LightKind {
    #[default]
    Directional,
    Point,
    Spot,
}

/// One light object in the scene.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Light3D {
    pub id: u64,
    pub name: String,
    pub enabled: bool,
    pub kind: LightKind,
    /// The light's colour, sRGB 0..1 like every colour param (1,1,1 = white).
    pub color: [f32; 3],
    /// Brightness multiplier.
    pub intensity: f32,
    /// Where the light is, in the object placement units: x/y in output pixels, z the camera
    /// distance (point and spot).
    pub position: [f32; 3],
    /// Aim as azimuth/elevation in degrees — the same numbers the classic light had, so a
    /// directional light at 135°/45° lights exactly like the old default (directional and spot).
    pub angle: f32,
    pub elevation: f32,
    /// Falloff radius in output pixels; 0 = the light reaches everything (point and spot).
    pub range: f32,
    /// The spot cone's outer half-angle in degrees.
    pub cone: f32,
    /// How much of the cone is a soft edge (0 = hard, 1 = all soft).
    pub softness: f32,
}

impl Default for Light3D {
    fn default() -> Self {
        Light3D {
            id: 0,
            name: "Light".into(),
            enabled: true,
            kind: LightKind::Directional,
            color: [1.0; 3],
            intensity: 1.0,
            position: [0.0, 0.0, 1500.0],
            angle: 135.0,
            elevation: 45.0,
            range: 0.0,
            cone: 35.0,
            softness: 0.5,
        }
    }
}

/// The lighting of a sequence's 3D layers. When `lights` is empty the renderers fall back to the
/// classic single light each object carries, so nothing changes for projects made before lights.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct SceneLighting {
    /// The colour that fills the shadows, multiplied by [`Self::ambient_level`].
    pub ambient_color: [f32; 3],
    pub ambient_level: f32,
    pub lights: Vec<Light3D>,
}

impl Default for SceneLighting {
    fn default() -> Self {
        SceneLighting { ambient_color: [1.0; 3], ambient_level: 1.0, lights: Vec::new() }
    }
}

impl SceneLighting {
    /// Nothing custom: keep the file small and the renderer on its classic single light.
    pub fn is_default(&self) -> bool {
        self.lights.is_empty() && self.ambient_level == 1.0 && self.ambient_color == [1.0; 3]
    }
}

/// The vertical field of view in degrees whose focal equals the frame height — the classic
/// fixed camera's (`focal = frame height` ⇔ `fov = 2·atan(0.5)`).
pub const CLASSIC_FOV: f32 = 53.130_102;

/// The app's rotation, matching the object/gizmo convention exactly (`Rx`, then `Ry`, then `Rz`):
/// `v' = Rz(Ry(Rx(v)))`. Angles in degrees.
fn rotate_xyz(v: [f32; 3], rx: f32, ry: f32, rz: f32) -> [f32; 3] {
    let (sx, cx) = rx.to_radians().sin_cos();
    let (sy, cy) = ry.to_radians().sin_cos();
    let (sz, cz) = rz.to_radians().sin_cos();
    let y = v[1] * cx - v[2] * sx;
    let z = v[1] * sx + v[2] * cx;
    let x = v[0] * cy + z * sy;
    let z = -v[0] * sy + z * cy;
    [x * cz - y * sz, x * sz + y * cz, z]
}

/// The camera a sequence's 3D layers render through, when one is added. Absent: the classic
/// fixed camera (frame-centred principal point, frame-height focal, looking down +Z from the
/// origin). Adding one reproduces the classic look exactly; moving it makes it a real camera.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct SceneCamera3D {
    /// Eye position in the scene's world units: frame pixels, frame-centred, at the object
    /// placement depth. x to the right, y down, z away from the viewer (the render direction).
    pub position: [f32; 3],
    /// Degrees: yaw around the vertical axis, pitch around the horizontal, roll around the view.
    pub yaw: f32,
    pub pitch: f32,
    pub roll: f32,
    /// Vertical field of view in degrees. The classic camera's is [`CLASSIC_FOV`].
    pub fov: f32,
}

impl Default for SceneCamera3D {
    fn default() -> Self {
        Self { position: [0.0; 3], yaw: 0.0, pitch: 0.0, roll: 0.0, fov: CLASSIC_FOV }
    }
}

impl SceneCamera3D {
    /// Whether this camera renders exactly like the classic fixed camera.
    pub fn is_classic(&self) -> bool {
        *self == Self::default()
    }

    /// Focal length in pixels for a frame `height` pixels tall.
    pub fn focal(&self, height: f32) -> f32 {
        (height * 0.5) / (self.fov.clamp(1.0, 179.0).to_radians() * 0.5).tan()
    }

    /// The camera's axes in world space: right, down, forward (row-major). The view rotation
    /// `Rᵀ` is these rows directly: `v' = [dot(right, w - eye), dot(down, w - eye),
    /// dot(forward, w - eye)]`.
    pub fn view_rotation(&self) -> [[f32; 3]; 3] {
        [
            rotate_xyz([1.0, 0.0, 0.0], self.pitch, self.yaw, self.roll),
            rotate_xyz([0.0, 1.0, 0.0], self.pitch, self.yaw, self.roll),
            rotate_xyz([0.0, 0.0, 1.0], self.pitch, self.yaw, self.roll),
        ]
    }

    /// The view matrix (world-to-camera), `[Rᵀ | -Rᵀ·eye]`. Column-major, ready for WGSL's
    /// `mat4x4`: `view * vec4(p, 1)` maps world to camera space.
    pub fn view_matrix(&self) -> [[f32; 4]; 4] {
        let vr = self.view_rotation();
        let t = [
            -(vr[0][0] * self.position[0] + vr[0][1] * self.position[1] + vr[0][2] * self.position[2]),
            -(vr[1][0] * self.position[0] + vr[1][1] * self.position[1] + vr[1][2] * self.position[2]),
            -(vr[2][0] * self.position[0] + vr[2][1] * self.position[1] + vr[2][2] * self.position[2]),
        ];
        // WGSL reads a mat4x4 as four columns; the columns are Vr's columns (Vr's rows are the
        // world-space camera axes).
        [
            [vr[0][0], vr[1][0], vr[2][0], 0.0],
            [vr[0][1], vr[1][1], vr[2][1], 0.0],
            [vr[0][2], vr[1][2], vr[2][2], 0.0],
            [t[0], t[1], t[2], 1.0],
        ]
    }
}
