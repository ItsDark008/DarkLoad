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

## Signed application updates

DarkLoad's updater only activates in builds that carry an updater public key. Set it up once:

1. Generate a key pair on a trusted machine. Choose a strong password when asked, and keep a backup of the key file and the password somewhere safe (for example a password manager): **if the key is lost, installed copies can never be updated again**, and anyone who has it can publish updates.

   ```powershell
   pnpm tauri signer generate -w "$HOME\.tauri\darkload.key"
   ```

2. Store the keys in the repository (GitHub CLI, signed in to the repository owner's account):

   ```powershell
   Get-Content "$HOME\.tauri\darkload.key" -Raw | gh secret set TAURI_SIGNING_PRIVATE_KEY
   ```

   ```powershell
   gh secret set TAURI_SIGNING_PRIVATE_KEY_PASSWORD
   ```

   ```powershell
   gh variable set TAURI_UPDATER_PUBKEY --body (Get-Content "$HOME\.tauri\darkload.key.pub" -Raw)
   ```

   The second command asks for the password interactively, so it never appears in your shell history.

3. Tag a release and **publish** the draft. The workflow signs every installer, writes `.sig` files and a `latest.json` manifest with one entry per installation type (`windows-x86_64-nsis`, `windows-x86_64-msi`, `linux-x86_64-appimage`, `-deb`, `-rpm`, plus `{os}-{arch}` fallbacks), and the app reads `https://github.com/<owner>/<repo>/releases/latest/download/latest.json`. Drafts are not "latest", so clients see an update only after publishing.

How the app behaves:

- It checks 20 seconds after startup and then every 6 hours, unless the user set **Settings → Updates** to "Off".
- With "Download and install" (the default) it downloads and verifies the update in the background, installs it when DarkLoad closes, and offers "Restart now". With "Only notify me" it only announces the version.
- Portable copies (`_portable.zip` / `.tar.gz`) and development builds cannot replace themselves; they only announce new versions with a link to the release.
- A build without the public key can never update. Version 1.0.0 was published that way, so users of 1.0.0 must install the next version manually once.

Without these values releases still build, but carry no update artifacts and the app states that automatic updates are not configured. **Never commit private keys.**

## Platform code signing

The workflow does not code-sign installers with an Authenticode certificate or Linux repository key, because no certificate is part of this repository. Windows SmartScreen may therefore warn on first run. To add Authenticode signing, configure `bundle.windows.certificateThumbprint` or a `signCommand` (see the Tauri documentation) using secrets on the runner. Do not describe releases as signed unless this is configured.
