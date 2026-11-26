# Releasing DarkLoad

Releases are built by GitHub Actions (`.github/workflows/release.yml`) from a version tag.

## Steps

1. Update the version in `package.json` and `src-tauri/Cargo.toml` (they must match), then run `cargo update -p darkload` in `src-tauri` to refresh `Cargo.lock`.
2. Move the `Unreleased` entries in `CHANGELOG.md` under a new `## [X.Y.Z] - YYYY-MM-DD` heading.
3. Commit, then tag and push:

   ```powershell
   git tag vX.Y.Z
   git push origin vX.Y.Z
   ```

4. The workflow builds Windows x64/ARM64 and Linux x64/ARM64, collects the installers under clear names, writes `SHA256SUMS.txt`, and creates a **draft** GitHub Release with the matching changelog section. Review the draft and publish it.

## Artifacts

| Platform | Files |
| --- | --- |
| Windows | `DarkLoad_<v>_windows_<arch>-setup.exe` (NSIS), `.msi`, `_portable.zip` |
| Linux | `DarkLoad_<v>_linux_<arch>.AppImage`, `.deb`, `.rpm`, `_portable.tar.gz` |

## Signed application updates (optional)

DarkLoad's updater only activates in builds that carry an updater public key. To enable it:

1. Generate a key pair on a trusted machine:

   ```powershell
   pnpm tauri signer generate -w "$HOME/.tauri/darkload.key"
   ```

2. In the repository settings add:
   - secret `TAURI_SIGNING_PRIVATE_KEY` — the contents of the private key file
   - secret `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` — its password (empty if none)
   - variable `TAURI_UPDATER_PUBKEY` — the contents of the `.pub` file

3. Tag a release. The workflow then produces `.sig` files and a `latest.json` manifest, and the app checks `https://github.com/<owner>/<repo>/releases/latest/download/latest.json`.

Without these values releases still build, but carry no update artifacts and the app states that automatic updates are not configured. **Never commit private keys.**

## Platform code signing

The workflow does not code-sign installers with an Authenticode certificate or Linux repository key, because no certificate is part of this repository. Windows SmartScreen may therefore warn on first run. To add Authenticode signing, configure `bundle.windows.certificateThumbprint` or a `signCommand` (see the Tauri documentation) using secrets on the runner. Do not describe releases as signed unless this is configured.
