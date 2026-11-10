//! Scene lights for 3D layers: the light objects a sequence is lit with (colored, multiple,
//! placeable in 3D), instead of one global light angle. An empty light list means the classic
//! per-object light from `light_angle`/`light_elevation`, so old projects render unchanged.

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
