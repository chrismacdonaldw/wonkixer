# Releasing

This is the maintainer runbook for producing and publishing a wonkixer release.
Release assets are immutable once published. Corrections ship under a new
version rather than replacing an existing tag or asset.

## Version

wonkixer follows semantic versioning. Keep the version synchronized in:

- `package.json` and `package-lock.json`
- `Cargo.toml` (`[workspace.package]`) and `Cargo.lock`

`src-tauri/tauri.conf.json` has no version of its own and inherits the Cargo
one. `scripts/build-release.ps1` refuses to build when these disagree.

## Preflight

```powershell
npm ci
npm run check
```

Review `git status`, the complete diff and the recent commits. Commit only the
files intended for the release; the release script refuses a dirty tree.

## Updater signing

Every installer is signed for the Tauri updater, and installed copies only
accept updates signed by the key whose public half is pinned in
`plugins.updater.pubkey` in `src-tauri/tauri.conf.json`. The key pair lives
outside the repository:

- `%USERPROFILE%\.tauri\wonkixer.key` (private, never commit or share it)
- `%USERPROFILE%\.tauri\wonkixer.key.pub` (must match the pinned public key)

The Tauri CLI signs during `tauri build` and reads two environment variables:

- `TAURI_SIGNING_PRIVATE_KEY`: the path to the private key, or its contents
- `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`: the key's password

The release script sets `TAURI_SIGNING_PRIVATE_KEY` to the standard path and
takes the password from the environment, then from a Windows-encrypted
`%USERPROFILE%\.tauri\wonkixer.key.password` if one exists. The current key was
generated without a password, so neither is needed and the CLI assumes an empty
one. To store a password for a protected key:

```powershell
Read-Host -AsSecureString | ConvertFrom-SecureString | Set-Content "$env:USERPROFILE\.tauri\wonkixer.key.password"
```

Back up the private key securely. Never generate a replacement after a signed
release has shipped: existing installations cannot trust updates signed by a
different key.

`bundle.createUpdaterArtifacts` is off in `tauri.conf.json`, so a plain
`npm run tauri build` needs no key and produces an unsigned installer for local
testing. The release script turns it on for its own build only by passing a
config override to the CLI:

```powershell
'{ "bundle": { "createUpdaterArtifacts": true } }' | Set-Content -Encoding ascii "$env:TEMP\wonkixer-release.conf.json"
npm run tauri build -- --config "$env:TEMP\wonkixer-release.conf.json"
```

The override goes through a file because Windows PowerShell 5.1 mangles the
quotes of an inline JSON argument.

With the override, the build fails at the signing step unless
`TAURI_SIGNING_PRIVATE_KEY` is set.

Updater signing does not Authenticode-sign the installer. Authenticode is a
separate certificate-based process and is not configured, so Windows
SmartScreen warns about the installer on first run.

## Build assets

```powershell
.\scripts\build-release.ps1 -Notes "Summary of this release"
```

The script checks the tree is clean, the versions agree and the signing key
matches the pinned public key, then runs `npm ci`, `npm run check` and
`npm run tauri build` with updater artifacts enabled and home-directory paths
remapped out of the binary.
The upload set is written to `target/release/release-assets/` at the
repository root (the Cargo workspace target, not `src-tauri/target`):

- `wonkixer_<version>_x64-setup.exe`, the per-user NSIS installer
- `wonkixer_<version>_x64-setup.exe.sig`, its updater signature
- `latest.json`, the updater manifest

`latest.json` carries the version, the notes, the publication time and, under
`platforms."windows-x86_64"`, the signature and the download URL of the
installer on the `v<version>` GitHub release. The script prints each asset's
size and SHA-256, and the exact `gh` command `-Publish` would run.

Pass `-AllowDirty` to build test assets from uncommitted changes. Such assets
must never be published, and the script refuses `-Publish` with `-AllowDirty`.

## Verify assets

Before publication:

1. Confirm `latest.json` has the intended version, notes, URL and signature.
2. Install the previous public release, then run the new installer over it.
   Confirm no administrator prompt appears and that devices, settings and user
   presets in `%APPDATA%\com.chrismacdonaldw.wonkixer` survive.
3. Start the engine with CABLE Input as the output and confirm Discord's mic
   test, with CABLE Output selected, plays the processed voice.
4. Uninstall from Windows Settings > Apps and reinstall to confirm a clean
   first run.

## Publish

Tag only after the tested source is committed:

```powershell
git tag -a vX.Y.Z -m "wonkixer vX.Y.Z"
git push origin main
git push origin vX.Y.Z
.\scripts\build-release.ps1 -Notes "Summary of this release" -Publish
```

With `-Publish` the script also requires the GitHub CLI to be authenticated,
the tag to exist and point at `HEAD`, and no release with that tag to exist
yet. It then creates the `vX.Y.Z` release with `gh release create --verify-tag`
(which refuses a tag that was not pushed) and uploads the installer, the
signature and `latest.json`.

Afterwards confirm the release is public, not a draft or prerelease, and is
the repository's latest release, and that
`https://github.com/chrismacdonaldw/wonkixer/releases/latest/download/latest.json`
returns the new version. That URL is the updater endpoint pinned in
`tauri.conf.json`, so every release must include its `latest.json`.
