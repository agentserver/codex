use crate::winutil::to_wide;
use std::path::Path;
use std::path::PathBuf;
#[cfg(target_os = "windows")]
use std::path::Prefix;
use windows_sys::Win32::Foundation::ERROR_MORE_DATA;
use windows_sys::Win32::Foundation::NO_ERROR;
use windows_sys::Win32::NetworkManagement::WNet::WNetGetConnectionW;

pub fn canonicalize_path(path: &Path) -> PathBuf {
    let mapped_path = resolve_sandbox_path(path);
    dunce::canonicalize(&mapped_path).unwrap_or(mapped_path)
}

pub fn normalize_spawn_cwd(path: &Path) -> PathBuf {
    let simplified = dunce::simplified(path).to_path_buf();
    if path_uses_unc_prefix(&simplified) {
        return simplified;
    }

    let canonical = dunce::canonicalize(path).ok();
    let canonical = canonical
        .as_deref()
        .map(dunce::simplified)
        .map(Path::to_path_buf);
    if let Some(canonical) = canonical
        && path_uses_unc_prefix(&canonical)
    {
        return canonical;
    }

    simplified
}

pub fn canonical_path_key(path: &Path) -> String {
    canonicalize_path(path)
        .to_string_lossy()
        .replace('\\', "/")
        .to_ascii_lowercase()
}

pub fn path_uses_unc_prefix(path: &Path) -> bool {
    #[cfg(target_os = "windows")]
    {
        matches!(
            path.components().next(),
            Some(std::path::Component::Prefix(prefix))
                if matches!(prefix.kind(), Prefix::UNC(..) | Prefix::VerbatimUNC(..))
        )
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = path;
        false
    }
}

pub fn resolve_sandbox_path(path: &Path) -> PathBuf {
    resolve_mapped_drive_path(path).unwrap_or_else(|| path.to_path_buf())
}

fn resolve_mapped_drive_path(path: &Path) -> Option<PathBuf> {
    let (drive, suffix) = split_mapped_drive_path(path)?;
    let drive_w = to_wide(drive);
    let mut remote_len = 0u32;
    let mut status =
        unsafe { WNetGetConnectionW(drive_w.as_ptr(), std::ptr::null_mut(), &mut remote_len) };
    if status != ERROR_MORE_DATA && status != NO_ERROR {
        return None;
    }

    let mut remote_buf = vec![0u16; remote_len as usize + 1];
    status =
        unsafe { WNetGetConnectionW(drive_w.as_ptr(), remote_buf.as_mut_ptr(), &mut remote_len) };
    if status != NO_ERROR {
        return None;
    }

    let remote_end = remote_buf
        .iter()
        .position(|ch| *ch == 0)
        .unwrap_or(remote_buf.len());
    let remote = String::from_utf16_lossy(&remote_buf[..remote_end]);
    if remote.is_empty() {
        return None;
    }

    let mut resolved = PathBuf::from(remote);
    if let Some(suffix) = suffix {
        resolved.push(suffix);
    }
    Some(resolved)
}

fn split_mapped_drive_path(path: &Path) -> Option<(&str, Option<&str>)> {
    let raw = path.to_str()?;
    let bytes = raw.as_bytes();
    if bytes.len() < 2 || !bytes[0].is_ascii_alphabetic() || bytes[1] != b':' {
        return None;
    }
    if bytes.len() > 2 && bytes[2] != b'\\' && bytes[2] != b'/' {
        return None;
    }

    let suffix = if bytes.len() > 3 {
        Some(&raw[3..])
    } else {
        None
    };
    Some((&raw[..2], suffix))
}

#[cfg(test)]
mod tests {
    use super::canonical_path_key;
    use super::normalize_spawn_cwd;
    use super::path_uses_unc_prefix;
    use super::split_mapped_drive_path;
    use pretty_assertions::assert_eq;
    use std::path::Path;
    use std::path::PathBuf;

    #[test]
    fn canonical_path_key_normalizes_case_and_separators() {
        let windows_style = Path::new(r"C:\Users\Dev\Repo");
        let slash_style = Path::new("c:/users/dev/repo");

        assert_eq!(
            canonical_path_key(windows_style),
            canonical_path_key(slash_style)
        );
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn path_uses_unc_prefix_matches_standard_and_verbatim_unc_paths() {
        assert!(path_uses_unc_prefix(Path::new(r"\\server\share\repo")));
        assert!(path_uses_unc_prefix(Path::new(
            r"\\?\UNC\server\share\repo"
        )));
        assert!(!path_uses_unc_prefix(Path::new(r"C:\repo")));
    }

    #[test]
    fn normalize_spawn_cwd_preserves_regular_local_paths() {
        let path = PathBuf::from(r"C:\repo");

        assert_eq!(normalize_spawn_cwd(&path), path);
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn normalize_spawn_cwd_simplifies_verbatim_unc_paths() {
        let path = PathBuf::from(r"\\?\UNC\server\share\repo");

        assert_eq!(
            normalize_spawn_cwd(&path),
            PathBuf::from(r"\\server\share\repo")
        );
    }

    #[test]
    fn split_mapped_drive_path_keeps_drive_relative_paths_unchanged() {
        assert_eq!(split_mapped_drive_path(Path::new(r"L:repo")), None);
    }

    #[test]
    fn split_mapped_drive_path_extracts_drive_root_suffix() {
        assert_eq!(
            split_mapped_drive_path(Path::new(r"L:\cs-web\context")),
            Some(("L:", Some("cs-web\\context")))
        );
    }
}
