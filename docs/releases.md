# Automated releases

Every push to `main` starts [the release workflow](../.github/workflows/release.yml).
It tests the workspace, computes a semantic version, builds native packages, and
publishes them to [GitHub Releases](https://github.com/Lvcky-gg/mtgo_rs/releases).
You can also run **Actions → Release → Run workflow** on `main` to release its
current commit. To retry an earlier commit, use **Re-run all jobs** on its original
workflow run. Manual runs on other branches are skipped.

## Versions

The highest stable `vMAJOR.MINOR.PATCH` tag is the baseline, with the workspace
version as the starting baseline before any releases. Messages in the push decide
the next version:

| Commit message | Bump | Example from 0.1.0 |
| --- | --- | --- |
| `feat: ...` or `feat(scope): ...` | Minor | 0.2.0 |
| `fix!: ...`, `feat(scope)!: ...`, or a `BREAKING CHANGE:` footer | Major | 1.0.0 |
| Other messages, including `fix:`, `docs:`, and ordinary messages | Patch | 0.1.1 |

The largest bump in the push wins. Every push gets a release, including changes
only to documentation. Prerelease tags do not affect stable version allocation.
Re-running a commit already tagged with a stable release reuses that version.

The build checkout's workspace version, local workspace entries in `Cargo.lock`,
macOS bundle versions and Flatpak release metadata are stamped with the computed
version. The workflow does not push a version-only commit back to `main`, so it
cannot cause a release loop. Tags point to the exact pushed commit; rebuilding
from a tag requires stamping the version using the helper below.

Pushes share a [queued concurrency group](https://docs.github.com/en/actions/how-tos/write-workflows/choose-when-workflows-run/control-workflow-concurrency).
Running releases are not canceled by newer pushes. GitHub currently limits the
queue to 100 pending runs. If that limit is reached, or a build fails, retry the
affected commit's workflow run. A failed build does not publish a release.

## Downloads and installation

Each release includes:

- `mtgo-rs-vVERSION-windows-x64.exe`: download and run on 64-bit Windows. The
  executable uses the static C runtime and opens without a console window.
- `mtgo-rs-vVERSION-macos-arm64.dmg`: Apple Silicon Macs. Open the disk image and
  drag **MTGO RS** into **Applications**.
- `mtgo-rs-vVERSION-macos-x64.dmg`: Intel Macs, installed the same way.
- `mtgo-rs-vVERSION-linux-x64.flatpak`: install on 64-bit Linux with Flatpak.
- `SHA256SUMS.txt`: SHA-256 checksums for all four downloads.

macOS bundles are ad-hoc signed. They are not Developer ID signed or notarized:
that requires Apple developer credentials, which this repository has not been
given. macOS can require an explicit approval in **System Settings → Privacy &
Security → Open Anyway**. Windows executables are also not Authenticode signed.

For Flatpak, install Flatpak through your distribution first, then run:

```sh
flatpak remote-add --user --if-not-exists flathub https://flathub.org/repo/flathub.flatpakrepo
flatpak install --user ./mtgo-rs-vVERSION-linux-x64.flatpak
flatpak run io.github.lvcky_gg.MtgoRs
```

Replace `VERSION` with the downloaded version. Later bundles can be installed
with the same command. This publishes downloadable bundles on GitHub; it does
not submit the application to Flathub or create an automatic-update remote.
Flatpak keeps decks, card data and settings in the application's own data area.
Card files are downloaded through the app; releases do not bundle a card database.

## Local files

Windows uses the current user's application-data folders, independently of the
executable's location or working directory:

| File | Windows default |
| --- | --- |
| Database, decks, identity and saved settings | `%LOCALAPPDATA%\mtgo_rs\cards.sqlite` |
| Downloaded card images and mana symbols | `%LOCALAPPDATA%\mtgo_rs\images` |
| Optional artwork name mappings | `%APPDATA%\mtgo_rs\art.txt` |

The folder choices follow [Windows application-data environment variables](https://learn.microsoft.com/en-us/windows/deployment/usmt/usmt-recognized-environment-variables).
If the application-data variables are missing, the client derives those folders
from `USERPROFILE`. Linux, Flatpak and existing macOS installations retain their
current XDG/HOME locations.

`MTGO_RS_DB` still overrides the database file. `XDG_DATA_HOME`, `XDG_CACHE_HOME`
and `XDG_CONFIG_HOME` still override the corresponding directories, including on
Windows. Empty variables are ignored, and non-Unicode filesystem paths are
preserved.

An older Windows build may have saved its database beneath `HOME\.local\share`
or its working directory. Existing databases are not moved automatically. To
retain an older Windows collection, close the app and copy its `mtgo_rs` data
folder to `%LOCALAPPDATA%\mtgo_rs`, keeping a backup, or point `MTGO_RS_DB` at
the original database. This change does not relocate existing Linux/macOS data.

## Repository setup

Enable GitHub Actions for the repository. The publish job requests
`contents: write` for its `GITHUB_TOKEN`; repository or organization policies must
permit it to create releases and tags. No personal access token, ngrok token,
Apple account, or external release service is required for these builds.

The workflow stages are:

1. Calculate the version from tags and the push's commit messages.
2. Run the release-helper tests, Rust formatting, workspace tests, and Clippy.
3. Build Windows and both macOS architectures on their native runners. Vendor
   dependencies for Linux, then compile offline inside Freedesktop SDK 25.08.
4. Verify all assets are present, generate checksums, upload to a draft release,
   then publish the complete release. Retries replace assets on the existing
   release for that commit.

## Local checks and rebuilds

```sh
python3 -m unittest discover -s scripts/tests -v
bash -n scripts/package-macos.sh scripts/prepare-flatpak.sh
desktop-file-validate packaging/flatpak/io.github.lvcky_gg.MtgoRs.desktop
appstreamcli validate --no-net packaging/flatpak/io.github.lvcky_gg.MtgoRs.metainfo.xml
```

Use a clean checkout when stamping or preparing packages. These commands change
the checkout's manifest, lockfile and packaging metadata:

```sh
python3 scripts/release.py stamp 0.1.1
cargo build --locked --release -p mtg-app --bin mtg-gui
```

On macOS, add `--target aarch64-apple-darwin` or
`--target x86_64-apple-darwin` to the build, then run, for example:

```sh
bash scripts/package-macos.sh 0.1.1 macos-arm64 aarch64-apple-darwin
```

On Linux, install `flatpak-builder`, Freedesktop Platform/SDK 25.08 and its
`org.freedesktop.Sdk.Extension.rust-stable` extension, then:

```sh
bash scripts/prepare-flatpak.sh
flatpak-builder --user --force-clean --repo=flatpak-repo build-dir packaging/flatpak/io.github.lvcky_gg.MtgoRs.json
flatpak build-bundle --runtime-repo=https://flathub.org/repo/flathub.flatpakrepo flatpak-repo dist/mtgo-rs-v0.1.1-linux-x64.flatpak io.github.lvcky_gg.MtgoRs stable
```

Preparing Flatpak sources downloads locked dependencies into `vendor/` and
creates `.cargo/config.toml` pointing Cargo at those sources. Keep this in a
dedicated build checkout. The archive excludes build outputs and Git metadata.
