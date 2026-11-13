# Unified Projects library

Home lists three kinds of entries, filtered with All / Videos / Drawings / Pictures:

- Existing video project folders, with unchanged `meta.json`/`project.fcproj` storage.
- Native ProCraft artworks from `files/artworks`, including artworks inside stacks.
  Opening one reopens its native layered document, not its thumbnail or a flattened copy.
- FilmCraft photo exports in MediaStore's `Pictures/FilmCraft` folder. This requires
  no new broad photo-library permission on Android 10+; only accessible/owned images
  are returned. The separate photo picker is used to import other photos.

Rename, duplicate, and delete dispatch by entry kind. A picture delete affects its
MediaStore image; a drawing delete affects the native artwork but not exported photos.
No artwork files are moved or converted. A temporary video-engine shell hosts standalone
Draw; it is marked hidden in project metadata and deleted on normal exit.

The drawing toolbar has a single host X (save and return), not a separate native gallery
button. All saved-library navigation is through Projects. The working upstream GPU and
brush-loading behaviour from 0.0.48 is unchanged.

The shared application skin uses ProCraft typography, neutral grey surfaces, blue
selected controls, rounded popovers/dialogs, and consistent 44dp icon targets. Home
uses a responsive thumbnail grid; settings use rounded grouped rows. Existing editor
backdrop blur remains untinted, without adding glass tints, outlines, or grain.

## Device acceptance checks (not substitutes for JVM/build tests)

- Existing video projects still open, rename, duplicate, delete, and export.
- A drawing saved in 0.0.48 appears under Drawings and All; reopening preserves layers.
- Artworks in an existing stack also appear; no files are migrated or deleted.
- Existing saved PNGs appear under Pictures, decode as bounded thumbnails, and open in Draw.
- Save a new drawing: layered artwork and published photo appear after returning Home.
- Rename/delete/duplicate one of each kind and confirm it affects only the chosen item.
- Import picture uses an image-only picker and opens the image in Draw, not a blank video.
- No standalone video-engine shell appears as a blank project.
- Check Home/settings/export/editor dialogs and toolbars in portrait and landscape.
