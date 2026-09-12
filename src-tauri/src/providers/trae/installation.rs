#[cfg(target_os = "linux")]
use std::fs;
#[cfg(target_os = "windows")]
use std::path::PathBuf;

const CN_WORK_BUNDLE_ID: &str = "cn.trae.solo.app";
const CN_CODE_BUNDLE_ID: &str = "cn.trae.app";

pub fn is_installed() -> bool {
    #[cfg(target_os = "macos")]
    {
        return any_bundle_id_matches(&[CN_WORK_BUNDLE_ID, CN_CODE_BUNDLE_ID], bundle_exists);
    }
    #[cfg(target_os = "windows")]
    {
        return known_install_paths().iter().any(|path| path.is_dir());
    }
    #[cfg(target_os = "linux")]
    {
        return linux_desktop_entries().iter().any(|path| {
            fs::read_to_string(path).is_ok_and(|text| {
                let text = text.to_ascii_lowercase();
                text.contains("trae") && (text.contains("traework") || text.contains("trae cn"))
            })
        });
    }
    #[allow(unreachable_code)]
    false
}

fn any_bundle_id_matches<F>(bundle_ids: &[&str], mut exists: F) -> bool
where
    F: FnMut(&str) -> bool,
{
    bundle_ids.iter().any(|bundle_id| exists(bundle_id))
}

#[cfg(target_os = "macos")]
fn bundle_exists(bundle_id: &str) -> bool {
    use std::process::Command;

    Command::new("mdfind")
        .arg(format!("kMDItemCFBundleIdentifier == '{bundle_id}'"))
        .output()
        .is_ok_and(|output| output.status.success() && !output.stdout.is_empty())
}

#[cfg(target_os = "windows")]
fn known_install_paths() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    for name in ["LOCALAPPDATA", "PROGRAMFILES", "PROGRAMFILES(X86)"] {
        if let Some(root) = std::env::var_os(name) {
            roots.push(PathBuf::from(root));
        }
    }
    let mut paths = Vec::new();
    for root in roots {
        for relative in [
            "Programs/Trae",
            "Programs/Trae CN",
            "Programs/TraeWork",
            "Trae",
            "Trae CN",
            "TraeWork",
            "TraeWork CN",
        ] {
            paths.push(root.join(relative));
        }
    }
    paths
}

#[cfg(target_os = "linux")]
fn linux_desktop_entries() -> Vec<PathBuf> {
    let mut directories = vec![PathBuf::from("/usr/share/applications")];
    if let Some(home) = std::env::var_os("HOME") {
        directories.push(PathBuf::from(home).join(".local/share/applications"));
    }
    directories
        .into_iter()
        .filter_map(|directory| fs::read_dir(directory).ok())
        .flat_map(|entries| entries.filter_map(Result::ok))
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == "desktop")
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::any_bundle_id_matches;

    #[test]
    fn accepts_either_supported_cn_bundle_id() {
        assert!(any_bundle_id_matches(
            &["cn.trae.solo.app", "cn.trae.app"],
            |bundle_id| bundle_id == "cn.trae.app"
        ));
    }

    #[test]
    fn rejects_international_or_unrelated_bundle_ids() {
        assert!(!any_bundle_id_matches(
            &["cn.trae.solo.app", "cn.trae.app"],
            |_| false
        ));
    }
}
