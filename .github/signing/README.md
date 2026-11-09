# Signing keys

## `dryrun-release.p12` — a public test key, not a secret

This keystore is **public on purpose**. Its private key is in this repository, so anyone can sign
an APK that Android considers "the same app" as one signed with it. It exists so a branch build or
a manual **Release › Run workflow** dry run produces a *release-signed* APK when the repository has
no signing secrets, instead of a debug-signed one.

| | |
|---|---|
| File | `dryrun-release.p12` (PKCS#12) |
| Alias | `filmcraft-dryrun` |
| Store and key password | `filmcraft-dryrun` |
| Subject | `CN=FilmCraft Dry Run, OU=Public CI test key, O=FilmCraft for Android` |
| Certificate SHA-256 | `b75617b60b055b3b02ff599744c69ddebe545a60ed305333f4fc442a9a27bcc0` |
| Key | RSA 3072, valid until 2054-03-27 |

Never use it for a tag: the Release workflow fails a tag build that has no real release key.

### Installing a dry-run APK

The application id is the same as the published app (`io.github.artcraftandroid.filmcraft`) but the
signing key is different, so Android refuses to install one over the other. **Uninstall the copy
you have first** (that deletes its projects), then install the dry-run APK. The same applies in
reverse: going back to a published build needs another uninstall.

## The published release key

Published releases are signed with the shared `artcraft-android` key, held outside this repository
in the `FILMCRAFT_KEYSTORE_BASE64`, `FILMCRAFT_KEYSTORE_PASSWORD`, `FILMCRAFT_KEY_ALIAS` and
`FILMCRAFT_KEY_PASSWORD` Actions secrets. When those secrets are present the workflow uses them for
dry runs too, and the artifact then updates installed copies normally.

`lightcraft-to-artcraft.lineage` is the public proof-of-rotation from the v0.0.1-beta key
(`CN=LightCraft for Android`) to the shared key. It is only used with the real key: a fresh key like
the dry-run one is not a rotation of anything, so it signs v2 + v3 with no lineage.

See [`docs/fdroid/README.md`](../../docs/fdroid/README.md) for the certificate pins and the release
procedure.
