//! Shared application directories for the GUI and command-line tools.
use std::{ffi::OsString, path::PathBuf};

#[derive(Clone, Copy)]
enum Directory {
    Data,
    Cache,
    Config,
}

/// Application data: XDG override, Windows LocalAppData, or the existing Unix path.
pub fn data_dir() -> PathBuf {
    app_dir(Directory::Data)
}

/// Disposable downloaded files: XDG override, Windows LocalAppData, or Unix cache.
pub fn cache_dir() -> PathBuf {
    app_dir(Directory::Cache)
}

/// User configuration: XDG override, Windows AppData, or the existing Unix path.
pub fn config_dir() -> PathBuf {
    app_dir(Directory::Config)
}

/// `MTGO_RS_DB` remains an explicit file path, including relative paths.
pub fn database_path() -> PathBuf {
    database_path_with(cfg!(windows), environment)
}

fn app_dir(kind: Directory) -> PathBuf {
    resolve(kind, cfg!(windows), environment)
}

fn environment(key: &str) -> Option<OsString> {
    std::env::var_os(key)
}

fn database_path_with(windows: bool, env: impl Fn(&str) -> Option<OsString>) -> PathBuf {
    nonempty(&env, "MTGO_RS_DB")
        .unwrap_or_else(|| resolve(Directory::Data, windows, env).join("cards.sqlite"))
}

fn nonempty(env: &impl Fn(&str) -> Option<OsString>, key: &str) -> Option<PathBuf> {
    env(key)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}

fn resolve(kind: Directory, windows: bool, env: impl Fn(&str) -> Option<OsString>) -> PathBuf {
    let (xdg, unix_suffix, windows_var, windows_suffix) = match kind {
        Directory::Data => ("XDG_DATA_HOME", ".local/share", "LOCALAPPDATA", "Local"),
        Directory::Cache => ("XDG_CACHE_HOME", ".cache", "LOCALAPPDATA", "Local"),
        Directory::Config => ("XDG_CONFIG_HOME", ".config", "APPDATA", "Roaming"),
    };
    let base = if let Some(explicit) = nonempty(&env, xdg) {
        explicit
    } else if windows {
        nonempty(&env, windows_var)
            .or_else(|| {
                nonempty(&env, "USERPROFILE")
                    .or_else(|| nonempty(&env, "HOME"))
                    .map(|home| home.join("AppData").join(windows_suffix))
            })
            // A stripped environment must still never write next to the EXE or
            // into its working directory. TEMP is user-specific on Windows.
            .unwrap_or_else(std::env::temp_dir)
    } else {
        // Keep existing Unix installations (including macOS) at the same path.
        nonempty(&env, "HOME")
            .unwrap_or_else(|| PathBuf::from("."))
            .join(unix_suffix)
    };
    base.join("mtgo_rs")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env(values: &[(&str, &str)]) -> impl Fn(&str) -> Option<OsString> {
        move |key| {
            values
                .iter()
                .find(|(name, _)| *name == key)
                .map(|(_, value)| value.into())
        }
    }

    #[test]
    fn windows_uses_native_data_cache_and_config_even_if_home_is_set() {
        let values = [
            ("LOCALAPPDATA", "local-app-data"),
            ("APPDATA", "roaming-app-data"),
            ("HOME", "shell-home"),
        ];
        assert_eq!(
            database_path_with(true, env(&values)),
            PathBuf::from("local-app-data/mtgo_rs/cards.sqlite")
        );
        assert_eq!(
            resolve(Directory::Cache, true, env(&values)),
            PathBuf::from("local-app-data/mtgo_rs")
        );
        assert_eq!(
            resolve(Directory::Config, true, env(&values)),
            PathBuf::from("roaming-app-data/mtgo_rs")
        );
    }

    #[test]
    fn windows_without_home_falls_back_to_the_user_profile() {
        let values = [
            ("USERPROFILE", "profile"),
            ("LOCALAPPDATA", ""),
            ("APPDATA", ""),
        ];
        assert_eq!(
            database_path_with(true, env(&values)),
            PathBuf::from("profile/AppData/Local/mtgo_rs/cards.sqlite")
        );
        assert_eq!(
            resolve(Directory::Config, true, env(&values)),
            PathBuf::from("profile/AppData/Roaming/mtgo_rs")
        );
    }

    #[test]
    fn windows_with_no_profile_never_falls_back_to_the_working_directory() {
        assert_eq!(
            database_path_with(true, env(&[])),
            std::env::temp_dir().join("mtgo_rs/cards.sqlite")
        );
    }

    #[test]
    fn explicit_database_and_xdg_overrides_keep_precedence_on_every_platform() {
        let values = [
            ("MTGO_RS_DB", "chosen.sqlite"),
            ("XDG_DATA_HOME", "xdg-data"),
            ("XDG_CACHE_HOME", "xdg-cache"),
            ("XDG_CONFIG_HOME", "xdg-config"),
            ("LOCALAPPDATA", "local"),
            ("APPDATA", "roaming"),
            ("HOME", "home"),
        ];
        for windows in [true, false] {
            assert_eq!(
                database_path_with(windows, env(&values)),
                PathBuf::from("chosen.sqlite")
            );
            assert_eq!(
                resolve(Directory::Data, windows, env(&values)),
                PathBuf::from("xdg-data/mtgo_rs")
            );
            assert_eq!(
                resolve(Directory::Cache, windows, env(&values)),
                PathBuf::from("xdg-cache/mtgo_rs")
            );
            assert_eq!(
                resolve(Directory::Config, windows, env(&values)),
                PathBuf::from("xdg-config/mtgo_rs")
            );
        }
    }

    #[test]
    fn unix_defaults_and_empty_overrides_preserve_existing_paths() {
        let values = [("HOME", "home"), ("XDG_DATA_HOME", ""), ("MTGO_RS_DB", "")];
        assert_eq!(
            database_path_with(false, env(&values)),
            PathBuf::from("home/.local/share/mtgo_rs/cards.sqlite")
        );
        assert_eq!(
            resolve(Directory::Cache, false, env(&values)),
            PathBuf::from("home/.cache/mtgo_rs")
        );
        assert_eq!(
            resolve(Directory::Config, false, env(&values)),
            PathBuf::from("home/.config/mtgo_rs")
        );
    }

    #[cfg(unix)]
    #[test]
    fn paths_preserve_non_unicode_environment_values() {
        use std::os::unix::ffi::OsStringExt;
        let path = OsString::from_vec(vec![b'/', b'x', 0xff]);
        assert_eq!(
            database_path_with(false, |key| (key == "MTGO_RS_DB").then(|| path.clone())),
            PathBuf::from(path)
        );
    }
}
