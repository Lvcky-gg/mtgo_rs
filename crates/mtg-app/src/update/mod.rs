//! Launch-time package updates. This module never opens or moves application data.
use std::{
    fs::{self, File},
    io::{Read, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::mpsc::{self, Receiver, Sender},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use serde::Deserialize;
use sha2::{Digest, Sha256};

pub mod window;

const REPOSITORY: &str = "https://github.com/Lvcky-gg/mtgo_rs/releases/download/";
const LATEST: &str = "https://api.github.com/repos/Lvcky-gg/mtgo_rs/releases/latest";
const APP_ID: &str = "io.github.lvcky_gg.MtgoRs";
const MAX_DOWNLOAD: u64 = 512 * 1024 * 1024;

#[derive(Deserialize)]
struct Release {
    tag_name: String,
    draft: bool,
    prerelease: bool,
    assets: Vec<Asset>,
}

#[derive(Deserialize)]
struct Asset {
    name: String,
    browser_download_url: String,
    size: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct Version(u64, u64, u64);

impl Version {
    fn parse(text: &str) -> Result<Self, String> {
        let parts: Vec<_> = text.strip_prefix('v').unwrap_or(text).split('.').collect();
        if parts.len() != 3 {
            return Err("Invalid stable release version".into());
        }
        let number = |s: &str| {
            if s.is_empty()
                || !s.bytes().all(|b| b.is_ascii_digit())
                || (s.len() > 1 && s.starts_with('0'))
            {
                return Err("Invalid stable release version".to_string());
            }
            s.parse::<u64>().map_err(|e| e.to_string())
        };
        Ok(Self(
            number(parts[0])?,
            number(parts[1])?,
            number(parts[2])?,
        ))
    }
}

pub enum Message {
    Progress(String),
    Finished(Result<Option<Restart>, String>),
}

/// Only verified package files can reach an installer.
pub enum Restart {
    Native {
        target: PathBuf,
        new: PathBuf,
        stage: PathBuf,
        macos: bool,
    },
    Flatpak,
}

/// Package builds opt in in the release workflow. Source/debug builds cannot overwrite
/// themselves with a packaged application while a developer is working.
pub fn start(ctx: egui::Context) -> Receiver<Message> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let result = match option_env!("MTGO_RS_RELEASE_PLATFORM") {
            Some(platform) => check(platform, &tx),
            None => Ok(None),
        };
        let _ = tx.send(Message::Finished(result));
        ctx.request_repaint();
    });
    rx
}

fn progress(tx: &Sender<Message>, text: impl Into<String>) {
    let _ = tx.send(Message::Progress(text.into()));
}

fn check(platform: &str, tx: &Sender<Message>) -> Result<Option<Restart>, String> {
    progress(tx, "Checking for updates…");
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(15)))
        .timeout_connect(Some(Duration::from_secs(8)))
        .build()
        .into();
    let body = get(&agent, LATEST, 2 * 1024 * 1024)?;
    let release: Release = serde_json::from_slice(&body).map_err(|e| e.to_string())?;
    let Some(asset) = release_update(&release, platform, env!("CARGO_PKG_VERSION"))? else {
        return Ok(None);
    };
    let checksums = release
        .assets
        .iter()
        .find(|a| a.name == "SHA256SUMS.txt")
        .ok_or("The release has no checksums")?;
    validate_url(checksums, &release.tag_name)?;
    let checksum_text = get(&agent, &checksums.browser_download_url, 64 * 1024)?;
    let expected = checksum(&checksum_text, &asset.name)?;
    let target = installation_target(
        platform,
        &std::env::current_exe().map_err(|e| e.to_string())?,
    )?;
    if let Some(target) = &target {
        protect_data_paths(
            target,
            platform.starts_with("macos-"),
            &[
                mtg_store::paths::database_path(),
                mtg_store::paths::data_dir(),
                mtg_store::paths::cache_dir(),
                mtg_store::paths::config_dir(),
            ],
        )?;
    }
    let base = if let Some(target) = &target {
        target
            .parent()
            .ok_or("Application has no parent folder")?
            .to_path_buf()
    } else {
        // Flatpak's persistent cache is visible at the same absolute path on the host.
        mtg_store::paths::cache_dir()
    };
    fs::create_dir_all(&base).map_err(|e| e.to_string())?;
    let base = base.canonicalize().map_err(|e| e.to_string())?;
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_nanos();
    let stage = base.join(format!(".mtgo-update-{}-{stamp}", std::process::id()));
    fs::create_dir(&stage).map_err(|e| format!("Cannot write the application folder: {e}"))?;
    progress(tx, format!("Downloading {}…", release.tag_name));
    let download = stage.join(&asset.name);
    if let Err(e) = download_verified(asset, &expected, &download) {
        let _ = fs::remove_file(&download);
        let _ = fs::remove_dir(&stage);
        return Err(e);
    }
    progress(tx, "Installing update…");
    if platform == "linux-x64-flatpak" {
        let result = Command::new("flatpak-spawn")
            .args([
                "--host",
                "flatpak",
                "install",
                "--user",
                "--noninteractive",
                "--assumeyes",
                "--or-update",
            ])
            .arg(&download)
            .output()
            .map_err(|e| e.to_string())?;
        if !result.status.success() {
            return Err(format!(
                "Flatpak update failed: {}",
                String::from_utf8_lossy(&result.stderr)
            ));
        }
        let _ = fs::remove_file(download);
        let _ = fs::remove_dir(stage);
        return Ok(Some(Restart::Flatpak));
    }
    let target = target.ok_or("Missing installation path")?;
    let macos = platform.starts_with("macos-");
    let new = if macos {
        let extracted = stage.join("unpacked");
        let result = Command::new("/usr/bin/ditto")
            .args(["-x", "-k"])
            .arg(&download)
            .arg(&extracted)
            .output()
            .map_err(|e| e.to_string())?;
        if !result.status.success() {
            return Err("Could not unpack the app update".into());
        }
        let app = extracted.join("MTGO RS.app");
        if !app.join("Contents/MacOS/mtg-gui").is_file() {
            return Err("Update is missing its executable".into());
        }
        let status = Command::new("/usr/bin/codesign")
            .args(["--verify", "--deep", "--strict"])
            .arg(&app)
            .status()
            .map_err(|e| e.to_string())?;
        if !status.success() {
            return Err("The app update failed signature verification".into());
        }
        app
    } else {
        download
    };
    Ok(Some(Restart::Native {
        target,
        new,
        stage,
        macos,
    }))
}

fn release_update<'a>(
    release: &'a Release,
    platform: &str,
    current: &str,
) -> Result<Option<&'a Asset>, String> {
    if release.draft
        || release.prerelease
        || Version::parse(&release.tag_name)? <= Version::parse(current)?
    {
        return Ok(None);
    }
    package(release, platform).map(Some)
}

fn package<'a>(release: &'a Release, platform: &str) -> Result<&'a Asset, String> {
    let suffix = match platform {
        "windows-x64" => "windows-x64.exe",
        "macos-arm64" => "macos-arm64.zip",
        "macos-x64" => "macos-x64.zip",
        "linux-x64-flatpak" => "linux-x64.flatpak",
        _ => return Err("Unsupported update platform".into()),
    };
    let name = format!("mtgo-rs-{}-{suffix}", release.tag_name);
    let asset = release
        .assets
        .iter()
        .find(|a| a.name == name)
        .ok_or("Release is missing this platform's update")?;
    validate_url(asset, &release.tag_name)?;
    if asset.size == 0 || asset.size > MAX_DOWNLOAD {
        return Err("Invalid update download size".into());
    }
    Ok(asset)
}

fn validate_url(asset: &Asset, tag: &str) -> Result<(), String> {
    if asset.browser_download_url != format!("{REPOSITORY}{tag}/{}", asset.name) {
        return Err("Update asset is outside the official release".into());
    }
    Ok(())
}

fn checksum(text: &[u8], name: &str) -> Result<[u8; 32], String> {
    let text = std::str::from_utf8(text).map_err(|e| e.to_string())?;
    let mut found = None;
    for line in text.lines() {
        let Some((hex, file)) = line.split_once("  ") else {
            continue;
        };
        if file != name {
            continue;
        }
        if found.is_some() || hex.len() != 64 || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err("Invalid release checksum".into());
        }
        let mut bytes = [0; 32];
        for (i, byte) in bytes.iter_mut().enumerate() {
            *byte = u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16).map_err(|e| e.to_string())?;
        }
        found = Some(bytes);
    }
    found.ok_or_else(|| "Update checksum is missing".into())
}

fn get(agent: &ureq::Agent, url: &str, limit: u64) -> Result<Vec<u8>, String> {
    let mut response = agent
        .get(url)
        .header("User-Agent", concat!("mtgo_rs/", env!("CARGO_PKG_VERSION")))
        .header("Accept", "application/vnd.github+json")
        .call()
        .map_err(|e| e.to_string())?;
    let mut bytes = Vec::new();
    response
        .body_mut()
        .as_reader()
        .take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() as u64 > limit {
        return Err("Update response is too large".into());
    }
    Ok(bytes)
}

fn download_verified(asset: &Asset, expected: &[u8; 32], path: &Path) -> Result<(), String> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(120)))
        .timeout_connect(Some(Duration::from_secs(8)))
        .build()
        .into();
    let mut response = agent
        .get(&asset.browser_download_url)
        .header("User-Agent", concat!("mtgo_rs/", env!("CARGO_PKG_VERSION")))
        .call()
        .map_err(|e| e.to_string())?;
    save_verified(response.body_mut().as_reader(), asset.size, expected, path)
}

fn save_verified(
    mut reader: impl Read,
    size: u64,
    expected: &[u8; 32],
    path: &Path,
) -> Result<(), String> {
    let mut file = File::create_new(path).map_err(|e| e.to_string())?;
    let mut hash = Sha256::new();
    let mut total = 0;
    let mut buffer = [0; 64 * 1024];
    loop {
        let n = reader.read(&mut buffer).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        total += n as u64;
        if total > size {
            return Err("Update exceeds its published size".into());
        }
        file.write_all(&buffer[..n]).map_err(|e| e.to_string())?;
        hash.update(&buffer[..n]);
    }
    if total != size || hash.finalize().as_slice() != expected {
        return Err("Update checksum or size does not match".into());
    }
    file.sync_all().map_err(|e| e.to_string())
}

// Reject a custom database/data path inside a bundle rather than swapping it away.
fn protect_data_paths(target: &Path, bundle: bool, paths: &[PathBuf]) -> Result<(), String> {
    let absolute = |path: &Path| -> Result<PathBuf, String> {
        if let Ok(path) = path.canonicalize() {
            return Ok(path);
        }
        let path = if path.is_absolute() {
            path.to_path_buf()
        } else {
            std::env::current_dir()
                .map_err(|e| e.to_string())?
                .join(path)
        };
        let mut normalized = PathBuf::new();
        for component in path.components() {
            match component {
                std::path::Component::CurDir => {}
                std::path::Component::ParentDir => {
                    normalized.pop();
                }
                other => normalized.push(other.as_os_str()),
            }
        }
        Ok(normalized)
    };
    let target = absolute(target)?;
    for path in paths {
        let path = absolute(path)?;
        if path == target || (bundle && path.starts_with(&target)) {
            return Err("Application data is inside the installation; keeping the current app to preserve it".into());
        }
    }
    Ok(())
}

fn installation_target(platform: &str, executable: &Path) -> Result<Option<PathBuf>, String> {
    if platform == "linux-x64-flatpak" {
        return Ok(None);
    }
    if platform.starts_with("macos-") {
        let contents = executable
            .parent()
            .and_then(Path::parent)
            .ok_or("Not running inside an app bundle")?;
        let app = contents
            .parent()
            .ok_or("Not running inside an app bundle")?;
        if contents.file_name().is_none_or(|n| n != "Contents")
            || app.extension().is_none_or(|e| e != "app")
        {
            return Err("Not running inside an app bundle".into());
        }
        return Ok(Some(app.to_path_buf()));
    }
    Ok(Some(executable.to_path_buf()))
}

fn restart_override(value: &std::ffi::OsStr, cwd: &Path) -> std::ffi::OsString {
    if value.is_empty() || Path::new(value).is_absolute() {
        value.to_os_string()
    } else {
        cwd.join(value).into_os_string()
    }
}

impl Restart {
    /// Called only after the update window has closed and its worker has finished.
    pub fn launch(self) -> Result<(), String> {
        match self {
            Self::Flatpak => {
                let mut command = Command::new("flatpak-spawn");
                command.args([
                    "--host",
                    "/bin/sh",
                    "-c",
                    "nohup flatpak run \"$@\" >/dev/null 2>&1 &",
                    "mtgo-update",
                    "--user",
                ]);
                for name in [
                    "MTGO_RS_DB",
                    "XDG_DATA_HOME",
                    "XDG_CONFIG_HOME",
                    "XDG_CACHE_HOME",
                ] {
                    if let Some(value) = std::env::var_os(name) {
                        let mut argument = std::ffi::OsString::from(format!("--env={name}="));
                        let cwd = std::env::current_dir().map_err(|e| e.to_string())?;
                        argument.push(restart_override(&value, &cwd));
                        command.arg(argument);
                    }
                }
                command.args([APP_ID, "--skip-update-once"]);
                let status = command.status().map_err(|e| e.to_string())?;
                if !status.success() {
                    return Err("Could not restart Flatpak".into());
                }
            }
            Self::Native {
                target,
                new,
                stage,
                macos,
            } => {
                let script = stage.join(if macos { "install.sh" } else { "install.ps1" });
                fs::write(
                    &script,
                    if macos {
                        include_str!("macos.sh")
                    } else {
                        include_str!("windows.ps1")
                    },
                )
                .map_err(|e| e.to_string())?;
                let mut command = Command::new(if macos { "/bin/sh" } else { "powershell.exe" });
                if macos {
                    command.arg(&script);
                } else {
                    command
                        .args([
                            "-NoProfile",
                            "-NonInteractive",
                            "-ExecutionPolicy",
                            "Bypass",
                            "-File",
                        ])
                        .arg(&script);
                    #[cfg(windows)]
                    {
                        use std::os::windows::process::CommandExt;
                        command.creation_flags(0x08000000); // CREATE_NO_WINDOW
                    }
                }
                command
                    .arg(target)
                    .arg(new)
                    .arg(stage)
                    .arg(std::process::id().to_string())
                    .arg(std::env::current_dir().map_err(|e| e.to_string())?)
                    .stdin(Stdio::null())
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .spawn()
                    .map_err(|e| e.to_string())?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn release() -> Release {
        let tag = "v0.12.3";
        Release {
            tag_name: tag.into(),
            draft: false,
            prerelease: false,
            assets: [
                "windows-x64.exe",
                "macos-arm64.zip",
                "macos-x64.zip",
                "linux-x64.flatpak",
            ]
            .map(|suffix| {
                let name = format!("mtgo-rs-{tag}-{suffix}");
                Asset {
                    browser_download_url: format!("{REPOSITORY}{tag}/{name}"),
                    name,
                    size: 100,
                }
            })
            .into(),
        }
    }

    #[test]
    fn versions_compare_numerically_and_reject_prerelease_or_malformed_tags() {
        assert!(Version::parse("v0.1.10").unwrap() > Version::parse("0.1.9").unwrap());
        assert!(Version::parse("1.0.0").unwrap() > Version::parse("0.99.99").unwrap());
        assert_eq!(Version::parse("v0.12.3"), Version::parse("0.12.3"));
        for bad in [
            "v1.2",
            "v1.2.3-rc.1",
            "1.2.3+build",
            "01.2.3",
            "1.2.3/../file",
            "1.2.3.4",
            "1.2.+3",
        ] {
            assert!(Version::parse(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn chooses_exact_platform_asset_and_rejects_foreign_or_incomplete_releases() {
        let mut release = release();
        for (platform, suffix) in [
            ("windows-x64", ".exe"),
            ("macos-arm64", "arm64.zip"),
            ("macos-x64", "x64.zip"),
            ("linux-x64-flatpak", ".flatpak"),
        ] {
            assert!(package(&release, platform).unwrap().name.ends_with(suffix));
        }
        assert!(package(&release, "linux-arm64").is_err());
        release.assets[0].browser_download_url = "https://example.invalid/malicious.exe".into();
        assert!(package(&release, "windows-x64").is_err());
        release.assets.remove(0);
        assert!(package(&release, "windows-x64").is_err());
    }

    #[test]
    fn only_newer_stable_complete_releases_are_installed() {
        let mut release = release();
        assert!(
            release_update(&release, "windows-x64", "0.12.2")
                .unwrap()
                .is_some()
        );
        assert!(
            release_update(&release, "windows-x64", "0.12.3")
                .unwrap()
                .is_none()
        );
        assert!(
            release_update(&release, "windows-x64", "0.13.0")
                .unwrap()
                .is_none()
        );
        release.draft = true;
        assert!(
            release_update(&release, "windows-x64", "0.12.2")
                .unwrap()
                .is_none()
        );
        release.draft = false;
        release.prerelease = true;
        assert!(
            release_update(&release, "windows-x64", "0.12.2")
                .unwrap()
                .is_none()
        );
        release.prerelease = false;
        release.assets.clear();
        assert!(release_update(&release, "windows-x64", "0.12.2").is_err());
        assert!(
            release_update(&release, "windows-x64", "0.12.3")
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn checksums_must_be_present_unique_and_valid_hex() {
        let digest = Sha256::digest(b"verified update");
        let hex: String = digest.iter().map(|b| format!("{b:02x}")).collect();
        let valid = format!("{hex}  client.exe\n");
        assert_eq!(
            checksum(valid.as_bytes(), "client.exe").unwrap().as_slice(),
            digest.as_slice()
        );
        assert!(checksum(valid.as_bytes(), "another.exe").is_err());
        assert!(checksum(format!("{valid}{valid}").as_bytes(), "client.exe").is_err());
        assert!(
            checksum(
                format!("{}  client.exe", "é".repeat(32)).as_bytes(),
                "client.exe"
            )
            .is_err()
        );
    }

    #[test]
    fn verified_download_refuses_corruption_truncation_excess_and_overwrites() {
        let dir = std::env::temp_dir().join(format!(
            "mtgo-update-download-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir(&dir).unwrap();
        let data = b"verified update";
        let expected: [u8; 32] = Sha256::digest(data).into();
        let good = dir.join("good.exe");
        save_verified(&data[..], data.len() as u64, &expected, &good).unwrap();
        assert_eq!(fs::read(&good).unwrap(), data);
        assert!(save_verified(&b"overwrite"[..], 9, &expected, &good).is_err());
        assert_eq!(fs::read(&good).unwrap(), data);
        for (name, bytes, size) in [
            ("corrupt", &b"wrong update!!!"[..], data.len() as u64),
            ("short", &data[..3], data.len() as u64),
            ("large", &data[..], 3),
        ] {
            assert!(save_verified(bytes, size, &expected, &dir.join(name)).is_err());
        }
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn flatpak_restart_keeps_empty_absolute_and_relative_database_overrides() {
        use std::ffi::OsStr;
        let cwd = std::env::temp_dir();
        assert_eq!(restart_override(OsStr::new(""), &cwd), OsStr::new(""));
        let absolute = cwd.join("existing/cards.sqlite");
        assert_eq!(
            restart_override(absolute.as_os_str(), &cwd),
            absolute.as_os_str()
        );
        assert_eq!(
            restart_override(OsStr::new("cards.sqlite"), &cwd),
            cwd.join("cards.sqlite").as_os_str()
        );
    }

    #[test]
    fn custom_database_inside_app_bundle_prevents_replacement() {
        let target = std::env::temp_dir().join("MTGO RS.app");
        assert!(
            protect_data_paths(&target, true, &[target.join("Contents/cards.sqlite")]).is_err()
        );
        assert!(
            protect_data_paths(&target, true, &[target.join("Contents/../cards.sqlite")]).is_err()
        );
        assert!(
            protect_data_paths(
                &target,
                true,
                &[std::env::temp_dir().join("mtgo-data/cards.sqlite")]
            )
            .is_ok()
        );
        assert!(protect_data_paths(&target, false, std::slice::from_ref(&target)).is_err());
    }

    #[test]
    fn native_target_is_only_the_executable_or_whole_mac_bundle() {
        assert_eq!(
            installation_target("windows-x64", Path::new("C:/apps/client.exe")).unwrap(),
            Some(PathBuf::from("C:/apps/client.exe"))
        );
        assert_eq!(
            installation_target(
                "macos-arm64",
                Path::new("/Applications/MTGO RS.app/Contents/MacOS/mtg-gui")
            )
            .unwrap(),
            Some(PathBuf::from("/Applications/MTGO RS.app"))
        );
        assert!(installation_target("macos-arm64", Path::new("/tmp/mtg-gui")).is_err());
        assert!(
            installation_target("linux-x64-flatpak", Path::new("/app/bin/mtg-gui"))
                .unwrap()
                .is_none()
        );
    }
}
