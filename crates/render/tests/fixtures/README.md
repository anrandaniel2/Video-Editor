# glTF test fixtures

Small, unmodified assets from [Khronos glTF-Sample-Assets](https://github.com/KhronosGroup/glTF-Sample-Assets),
used only by the `tests/` suites (not bundled in the app).

- `RiggedSimple.glb`, `RiggedFigure.glb`, `CesiumMan.glb` and `BoxTextured.glb`: Cesium, 2017,
  CC-BY-4.0. https://creativecommons.org/licenses/by/4.0/
  CesiumMan and BoxTextured also carry the Cesium logo/legal mark under the repository's
  `LICENSES/LicenseRef-LegalMark-Cesium.txt` (included here).
- `AnimatedMorphCube.glb`: CC0-1.0.
- `solid_4096x4097.png`: a 4096×4097 solid-colour PNG generated for these tests (CC0-1.0); it
  exercises the "no texture size ceiling" regression test.

The rigged fixtures are the rigging-oracle inputs (`tests/rigging.rs`): `RiggedSimple.glb` has a
two-joint nested chain, `RiggedFigure.glb` and `CesiumMan.glb` have full 20-joint human rigs with
LINEAR translation/rotation/scale animations.

No changes were made to the model files.
