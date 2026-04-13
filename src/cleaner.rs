use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use colored::Colorize;
use tracing::{debug, error, info, warn};

/// Result of a cleanup operation.
#[derive(Debug, Clone, Default)]
pub struct CleanResult {
    /// Total size in bytes of files cleaned.
    pub size: u64,
    /// Number of files removed.
    pub files_removed: u64,
    /// Number of directories removed.
    pub dirs_removed: u64,
}

impl CleanResult {
    /// Returns an empty result with all counters at zero.
    pub fn empty() -> Self {
        Self::default()
    }
}

/// Format a byte count into a human-readable string.
pub fn format_size(size: u64) -> String {
    if size == 0 {
        return "0B".to_string();
    }
    if size < 1024 {
        format!("{}B", size)
    } else if size < 1_048_576 {
        format!("{:.1}KB", size as f64 / 1024.0)
    } else if size < 1_073_741_824 {
        format!("{:.1}MB", size as f64 / 1_048_576.0)
    } else {
        format!("{:.2}GB", size as f64 / 1_073_741_824.0)
    }
}

/// Check if a path contains any protected directory component.
pub fn is_protected_path(path: &Path, protected_dirs: &[String]) -> bool {
    for component in path.components() {
        if let std::path::Component::Normal(name) = component {
            let name_str = name.to_string_lossy();
            if protected_dirs
                .iter()
                .any(|d| d.as_str() == name_str.as_ref())
            {
                return true;
            }
        }
    }
    false
}

fn file_age_days(path: &Path) -> Option<u64> {
    let metadata = fs::metadata(path).ok()?;
    let modified = metadata.modified().ok()?;
    let duration = modified.duration_since(UNIX_EPOCH).ok()?;
    let now = SystemTime::now().duration_since(UNIX_EPOCH).ok()?;
    Some(now.saturating_sub(duration).as_secs() / 86400)
}

fn scan_directory(path: &Path, max_age_days: Option<u64>) -> (u64, u64, Vec<PathBuf>) {
    let mut total_size: u64 = 0;
    let mut file_count: u64 = 0;
    let mut files_to_remove: Vec<PathBuf> = Vec::new();

    fn walk(
        dir: &Path,
        total_size: &mut u64,
        file_count: &mut u64,
        files_to_remove: &mut Vec<PathBuf>,
        max_age_days: Option<u64>,
    ) {
        if let Ok(entries) = fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    walk(
                        path.as_path(),
                        total_size,
                        file_count,
                        files_to_remove,
                        max_age_days,
                    );
                } else if let Ok(metadata) = fs::metadata(&path) {
                    let size = metadata.len();
                    *total_size += size;
                    *file_count += 1;

                    if let Some(max_age) = max_age_days {
                        if let Some(age) = file_age_days(&path) {
                            if age >= max_age {
                                files_to_remove.push(path);
                            }
                        }
                    } else {
                        files_to_remove.push(path);
                    }
                }
            }
        }
    }

    walk(
        path,
        &mut total_size,
        &mut file_count,
        &mut files_to_remove,
        max_age_days,
    );
    (total_size, file_count, files_to_remove)
}

/// Clean files in a directory, respecting age thresholds and protected paths.
pub fn clean_directory(
    path: &Path,
    dry_run: bool,
    protected_dirs: &[String],
    max_age_days: Option<u64>,
    verbose: bool,
) -> CleanResult {
    if !path.exists() {
        debug!(path = %path.display(), "Path does not exist, skipping");
        return CleanResult::empty();
    }

    if is_protected_path(path, protected_dirs) {
        warn!(path = %path.display(), "Protected path skipped");
        if verbose {
            eprintln!(
                "  [{}] Protected path skipped: {}",
                "skip".yellow(),
                path.display()
            );
        }
        return CleanResult::empty();
    }

    let (size, _file_count, files_to_remove) = scan_directory(path, max_age_days);

    if size == 0 && files_to_remove.is_empty() {
        return CleanResult::empty();
    }

    if dry_run {
        info!(
            path = %path.display(),
            files = files_to_remove.len(),
            size,
            "Would clean (dry run)"
        );
        if verbose {
            eprintln!(
                "  [{}] Would remove {} files ({}) from {}",
                "dry".blue(),
                files_to_remove.len(),
                format_size(size),
                path.display()
            );
        }
        return CleanResult {
            size,
            files_removed: files_to_remove.len() as u64,
            dirs_removed: 0,
        };
    }

    let mut removed_count: u64 = 0;
    for file_path in &files_to_remove {
        match fs::remove_file(file_path) {
            Ok(()) => removed_count += 1,
            Err(e) => {
                error!(path = %file_path.display(), error = %e, "Failed to remove file");
                if verbose {
                    eprintln!(
                        "  [{}] Failed to remove {}: {}",
                        "error".red(),
                        file_path.display(),
                        e
                    );
                }
            }
        }
    }

    if let Ok(entries) = fs::read_dir(path) {
        for entry in entries.flatten() {
            let p = entry.path();
            if p.is_dir() {
                if let Err(e) = fs::remove_dir_all(&p) {
                    error!(path = %p.display(), error = %e, "Failed to remove directory");
                    if verbose {
                        eprintln!(
                            "  [{}] Failed to remove dir {}: {}",
                            "error".red(),
                            p.display(),
                            e
                        );
                    }
                }
            }
        }
    }

    info!(path = %path.display(), size, removed = removed_count, "Cleaned directory");

    CleanResult {
        size,
        files_removed: removed_count,
        dirs_removed: 0,
    }
}

/// Recursively remove empty directories, respecting protected paths.
pub fn remove_empty_dirs(
    path: &Path,
    dry_run: bool,
    protected_dirs: &[String],
    verbose: bool,
) -> CleanResult {
    if !path.exists() {
        debug!(path = %path.display(), "Path does not exist, skipping");
        return CleanResult::empty();
    }

    if is_protected_path(path, protected_dirs) {
        return CleanResult::empty();
    }

    let mut dirs_removed: u64 = 0;

    fn walk_and_remove(
        dir: &Path,
        dry_run: bool,
        protected_dirs: &[String],
        dirs_removed: &mut u64,
        verbose: bool,
    ) -> bool {
        if let Ok(entries) = fs::read_dir(dir) {
            let mut all_removed = true;
            for entry in entries.flatten() {
                let entry_path = entry.path();
                if entry_path.is_dir() {
                    let is_protected = entry_path
                        .file_name()
                        .map(|n| {
                            protected_dirs
                                .iter()
                                .any(|d| d.as_str() == n.to_string_lossy().as_ref())
                        })
                        .unwrap_or(false);

                    if is_protected
                        || !walk_and_remove(
                            &entry_path,
                            dry_run,
                            protected_dirs,
                            dirs_removed,
                            verbose,
                        )
                    {
                        all_removed = false;
                    }
                } else {
                    all_removed = false;
                }
            }
            if all_removed && !dry_run {
                match fs::remove_dir(dir) {
                    Ok(()) => {
                        *dirs_removed += 1;
                        debug!(path = %dir.display(), "Removed empty directory");
                    }
                    Err(e) => {
                        error!(path = %dir.display(), error = %e, "Failed to remove empty directory");
                        if verbose {
                            eprintln!(
                                "  [{}] Failed to remove empty dir {}: {}",
                                "error".red(),
                                dir.display(),
                                e
                            );
                        }
                        return false;
                    }
                }
            }
            return all_removed;
        }
        false
    }

    walk_and_remove(path, dry_run, protected_dirs, &mut dirs_removed, verbose);

    if dirs_removed > 0 {
        info!(path = %path.display(), removed = dirs_removed, "Removed empty directories");
    }

    CleanResult {
        size: 0,
        files_removed: 0,
        dirs_removed,
    }
}

/// Remove orphan packages using pacman (Arch Linux only).
pub fn clean_orphans(dry_run: bool, verbose: bool) -> CleanResult {
    let output = std::process::Command::new("pacman")
        .args(["-Qdtq"])
        .output();

    match output {
        Ok(out) if out.status.success() => {
            let orphans = String::from_utf8_lossy(&out.stdout);
            if orphans.trim().is_empty() {
                debug!("No orphan packages found");
                return CleanResult::empty();
            }
            let lines: Vec<&str> = orphans.lines().collect();
            let count = lines.len() as u64;

            if verbose {
                if dry_run {
                    eprintln!(
                        "  [{}] Would remove {} orphan packages:",
                        "dry".blue(),
                        count
                    );
                } else {
                    eprintln!(
                        "  [{}] Removing {} orphan packages...",
                        "info".green(),
                        count
                    );
                }
                for pkg in &lines {
                    eprintln!("    - {}", pkg);
                }
            }

            if !dry_run {
                let result = std::process::Command::new("pkexec")
                    .args(["pacman", "-Rns", "--noconfirm"])
                    .args(&lines)
                    .output();

                if let Err(e) = result {
                    error!(error = %e, "Failed to remove orphan packages");
                    if verbose {
                        eprintln!("  [{}] Failed to remove orphans: {}", "error".red(), e);
                    }
                    return CleanResult::empty();
                }
            }

            info!(count, "Orphan packages processed");

            CleanResult {
                size: count * 1024 * 100,
                files_removed: count,
                dirs_removed: 0,
            }
        }
        Ok(out) => {
            let stderr = String::from_utf8_lossy(&out.stderr);
            debug!(stderr = %stderr.trim(), "pacman returned no orphans or error");
            if verbose {
                eprintln!(
                    "  [{}] pacman returned no orphans or error: {}",
                    "skip".yellow(),
                    stderr.trim()
                );
            }
            CleanResult::empty()
        }
        Err(e) => {
            error!(error = %e, "Failed to run pacman");
            if verbose {
                eprintln!("  [{}] Failed to run pacman: {}", "error".red(), e);
            }
            CleanResult::empty()
        }
    }
}

/// Remove a lock file if it exists.
pub fn remove_lock_file(path: &Path, dry_run: bool, verbose: bool) -> CleanResult {
    if path.exists() {
        let size = path.metadata().map(|m| m.len()).unwrap_or(0);
        if dry_run {
            debug!(path = %path.display(), "Would remove lock file (dry run)");
            if verbose {
                eprintln!(
                    "  [{}] Would remove lock file: {}",
                    "dry".blue(),
                    path.display()
                );
            }
        } else {
            match fs::remove_file(path) {
                Ok(()) => {
                    info!(path = %path.display(), "Removed lock file");
                    if verbose {
                        eprintln!("  [{}] Removed lock file: {}", "ok".green(), path.display());
                    }
                }
                Err(e) => {
                    error!(path = %path.display(), error = %e, "Failed to remove lock file");
                    if verbose {
                        eprintln!(
                            "  [{}] Failed to remove lock file {}: {}",
                            "error".red(),
                            path.display(),
                            e
                        );
                    }
                    return CleanResult::empty();
                }
            }
        }
        CleanResult {
            size,
            files_removed: 1,
            dirs_removed: 0,
        }
    } else {
        CleanResult::empty()
    }
}

/// Remove a single file if it exists.
pub fn remove_file(path: &Path, dry_run: bool, verbose: bool) -> CleanResult {
    if path.exists() {
        let size = path.metadata().map(|m| m.len()).unwrap_or(0);
        if dry_run {
            if verbose {
                eprintln!("  [{}] Would remove: {}", "dry".blue(), path.display());
            }
        } else {
            match fs::remove_file(path) {
                Ok(()) => {
                    if verbose {
                        eprintln!("  [{}] Removed: {}", "ok".green(), path.display());
                    }
                }
                Err(e) => {
                    error!(path = %path.display(), error = %e, "Failed to remove file");
                    if verbose {
                        eprintln!(
                            "  [{}] Failed to remove {}: {}",
                            "error".red(),
                            path.display(),
                            e
                        );
                    }
                    return CleanResult::empty();
                }
            }
        }
        CleanResult {
            size,
            files_removed: 1,
            dirs_removed: 0,
        }
    } else {
        CleanResult::empty()
    }
}

/// Run docker system prune.
pub fn docker_prune(dry_run: bool, verbose: bool) -> CleanResult {
    let output = std::process::Command::new("docker")
        .args(["system", "df", "--format", "{{.Size}}"])
        .output();

    let reclaimable_size: u64 = match output {
        Ok(out) if out.status.success() => {
            let stdout = String::from_utf8_lossy(&out.stdout);
            let lines: Vec<&str> = stdout.lines().collect();
            if lines.len() >= 4 {
                let total_line = lines[0].split_whitespace().next().unwrap_or("0");
                parse_docker_size(total_line)
            } else {
                0
            }
        }
        _ => 0,
    };

    if reclaimable_size == 0 && !dry_run {
        return CleanResult::empty();
    }

    if dry_run {
        if verbose {
            eprintln!(
                "  [{}] Would reclaim ~{} with docker prune",
                "dry".blue(),
                format_size(reclaimable_size)
            );
        }
        return CleanResult {
            size: reclaimable_size,
            files_removed: 0,
            dirs_removed: 0,
        };
    }

    let result = std::process::Command::new("docker")
        .args(["system", "prune", "-af", "--volumes"])
        .output();

    match result {
        Ok(out) if out.status.success() => {
            if verbose {
                eprintln!("  [{}] Docker prune completed", "ok".green());
            }
            CleanResult {
                size: reclaimable_size,
                files_removed: 1,
                dirs_removed: 0,
            }
        }
        Ok(out) => {
            let stderr = String::from_utf8_lossy(&out.stderr);
            error!(error = %stderr, "Docker prune failed");
            if verbose {
                eprintln!(
                    "  [{}] Docker prune failed: {}",
                    "error".red(),
                    stderr.trim()
                );
            }
            CleanResult::empty()
        }
        Err(e) => {
            error!(error = %e, "Failed to run docker");
            CleanResult::empty()
        }
    }
}

fn parse_docker_size(s: &str) -> u64 {
    let s = s.trim();
    if let Some(val) = s.strip_suffix("GB") {
        val.trim()
            .parse::<f64>()
            .ok()
            .map(|v| (v * 1_073_741_824.0) as u64)
            .unwrap_or(0)
    } else if let Some(val) = s.strip_suffix("MB") {
        val.trim()
            .parse::<f64>()
            .ok()
            .map(|v| (v * 1_048_576.0) as u64)
            .unwrap_or(0)
    } else if let Some(val) = s.strip_suffix("KB") {
        val.trim()
            .parse::<f64>()
            .ok()
            .map(|v| (v * 1024.0) as u64)
            .unwrap_or(0)
    } else if let Some(val) = s.strip_suffix("B") {
        val.trim().parse::<u64>().unwrap_or(0)
    } else {
        0
    }
}

/// Remove old pacman package versions, keeping only the last `keep` versions.
pub fn clean_pacman_old_packages(keep: usize, dry_run: bool, verbose: bool) -> CleanResult {
    let pkg_cache = PathBuf::from("/var/cache/pacman/pkg");
    if !pkg_cache.exists() {
        return CleanResult::empty();
    }

    let mut total_size: u64 = 0;
    let mut files_removed: u64 = 0;

    let entries = match fs::read_dir(&pkg_cache) {
        Ok(e) => e,
        Err(_) => return CleanResult::empty(),
    };

    let mut packages: std::collections::HashMap<String, Vec<(PathBuf, u64)>> =
        std::collections::HashMap::new();

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let filename = path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        if !filename.ends_with(".pkg.tar.zst") && !filename.ends_with(".pkg.tar.xz") {
            continue;
        }
        let size = path.metadata().map(|m| m.len()).unwrap_or(0);
        let pkg_name = extract_pacman_package_name(&filename);
        packages.entry(pkg_name).or_default().push((path, size));
    }

    for (_pkg, mut versions) in packages {
        if versions.len() <= keep {
            continue;
        }
        versions.sort_by(|a, b| b.0.cmp(&a.0));
        for (path, size) in versions.iter().skip(keep) {
            total_size += size;
            files_removed += 1;
            if !dry_run {
                if let Err(e) = fs::remove_file(path) {
                    error!(path = %path.display(), error = %e, "Failed to remove pacman package");
                }
            }
        }
    }

    if files_removed > 0 && verbose {
        eprintln!(
            "  [{}] {} old pacman package versions ({})",
            if dry_run { "dry".blue() } else { "ok".green() },
            files_removed,
            format_size(total_size)
        );
    }

    CleanResult {
        size: total_size,
        files_removed,
        dirs_removed: 0,
    }
}

/// Remove failed pacman download artifacts (download-* dirs/files).
pub fn clean_pacman_download_cache(dry_run: bool, verbose: bool) -> CleanResult {
    let pkg_cache = PathBuf::from("/var/cache/pacman/pkg");
    if !pkg_cache.exists() {
        return CleanResult::empty();
    }

    let mut total_size: u64 = 0;
    let mut files_removed: u64 = 0;
    let mut dirs_removed: u64 = 0;

    let entries = match fs::read_dir(&pkg_cache) {
        Ok(e) => e,
        Err(_) => return CleanResult::empty(),
    };

    for entry in entries.flatten() {
        let path = entry.path();
        let name = match path.file_name().and_then(|n| n.to_str()) {
            Some(n) => n,
            None => continue,
        };
        if !name.starts_with("download-") {
            continue;
        }
        let size = path.metadata().map(|m| m.len()).unwrap_or(0);
        total_size += size;
        if !dry_run {
            if path.is_dir() {
                if let Err(e) = fs::remove_dir_all(&path) {
                    error!(path = %path.display(), error = %e, "Failed to remove pacman download dir");
                    continue;
                }
                dirs_removed += 1;
            } else {
                if let Err(e) = fs::remove_file(&path) {
                    error!(path = %path.display(), error = %e, "Failed to remove pacman download file");
                    continue;
                }
                files_removed += 1;
            }
        } else {
            if path.is_dir() {
                dirs_removed += 1;
            } else {
                files_removed += 1;
            }
        }
    }

    if (files_removed > 0 || dirs_removed > 0) && verbose {
        eprintln!(
            "  [{}] {} pacman download artifacts ({})",
            if dry_run { "dry".blue() } else { "ok".green() },
            files_removed + dirs_removed,
            format_size(total_size)
        );
    }

    CleanResult {
        size: total_size,
        files_removed,
        dirs_removed,
    }
}

fn extract_pacman_package_name(filename: &str) -> String {
    filename
        .split_once('-')
        .map(|(name, _)| name.to_string())
        .unwrap_or_else(|| filename.to_string())
}

/// Clean journal log files.
pub fn clean_journal_logs(dry_run: bool, _protected_dirs: &[String], verbose: bool) -> CleanResult {
    let journal_path = PathBuf::from("/var/log/journal");
    if !journal_path.exists() {
        debug!("Journal log path does not exist, skipping");
        return CleanResult::empty();
    }

    let mut total_size: u64 = 0;
    let mut files_removed: u64 = 0;

    if let Ok(entries) = fs::read_dir(&journal_path) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                if let Ok(sub_entries) = fs::read_dir(&path) {
                    for sub_entry in sub_entries.flatten() {
                        let file_path = sub_entry.path();
                        if let Ok(metadata) = fs::metadata(&file_path) {
                            total_size += metadata.len();
                            files_removed += 1;

                            if !dry_run {
                                if let Err(e) = fs::remove_file(&file_path) {
                                    error!(path = %file_path.display(), error = %e, "Failed to remove journal file");
                                    if verbose {
                                        eprintln!(
                                            "  [{}] Failed to remove journal file {}: {}",
                                            "error".red(),
                                            file_path.display(),
                                            e
                                        );
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    if total_size > 0 {
        if dry_run {
            info!(size = total_size, "Would clean journal logs (dry run)");
        } else {
            info!(
                size = total_size,
                files = files_removed,
                "Cleaned journal logs"
            );
        }
        if verbose {
            if dry_run {
                eprintln!(
                    "  [{}] Would clean {} from journal logs",
                    "dry".blue(),
                    format_size(total_size)
                );
            } else {
                eprintln!(
                    "  [{}] Cleaned {} from journal logs",
                    "ok".green(),
                    format_size(total_size)
                );
            }
        }
    }

    CleanResult {
        size: total_size,
        files_removed,
        dirs_removed: 0,
    }
}

/// Clean system log files, only removing rotated or old files.
pub fn clean_system_logs(
    path: &Path,
    dry_run: bool,
    max_age_days: u64,
    protected_dirs: &[String],
    verbose: bool,
) -> CleanResult {
    if !path.exists() {
        debug!(path = %path.display(), "Log path does not exist, skipping");
        return CleanResult::empty();
    }

    if is_protected_path(path, protected_dirs) {
        return CleanResult::empty();
    }

    let mut total_size: u64 = 0;
    let mut files_removed: u64 = 0;

    fn walk_logs(
        dir: &Path,
        total_size: &mut u64,
        files_removed: &mut u64,
        dry_run: bool,
        max_age_days: u64,
        verbose: bool,
    ) {
        if let Ok(entries) = fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    walk_logs(
                        &path,
                        total_size,
                        files_removed,
                        dry_run,
                        max_age_days,
                        verbose,
                    );
                } else if let Ok(metadata) = fs::metadata(&path) {
                    if let Some(age) = file_age_days(&path) {
                        if age >= max_age_days {
                            let is_rotated = path
                                .file_name()
                                .map(|n| {
                                    let s = n.to_string_lossy();
                                    s.contains(".gz")
                                        || s.contains(".xz")
                                        || s.contains(".bz2")
                                        || s.contains(".old")
                                        || s.contains(".1")
                                })
                                .unwrap_or(false);

                            if is_rotated || age >= max_age_days * 2 {
                                *total_size += metadata.len();
                                *files_removed += 1;

                                if !dry_run {
                                    if let Err(e) = fs::remove_file(&path) {
                                        error!(path = %path.display(), error = %e, "Failed to remove log file");
                                        if verbose {
                                            eprintln!(
                                                "  [{}] Failed to remove log {}: {}",
                                                "error".red(),
                                                path.display(),
                                                e
                                            );
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    walk_logs(
        path,
        &mut total_size,
        &mut files_removed,
        dry_run,
        max_age_days,
        verbose,
    );

    if total_size > 0 {
        info!(path = %path.display(), size = total_size, files = files_removed, "Cleaned system logs");
    }

    CleanResult {
        size: total_size,
        files_removed,
        dirs_removed: 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::io::Write;
    use tempfile::TempDir;

    fn setup_test_dir() -> TempDir {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("test.txt");
        let mut file = File::create(&file_path).unwrap();
        file.write_all(b"hello world").unwrap();
        dir
    }

    #[test]
    fn test_format_size_bytes() {
        assert_eq!(format_size(0), "0B");
        assert_eq!(format_size(500), "500B");
    }

    #[test]
    fn test_format_size_kb() {
        let result = format_size(1500);
        assert!(result.contains("KB"));
    }

    #[test]
    fn test_format_size_mb() {
        let result = format_size(2_000_000);
        assert!(result.contains("MB"));
    }

    #[test]
    fn test_format_size_gb() {
        let result = format_size(2_000_000_000);
        assert!(result.contains("GB"));
    }

    #[test]
    fn test_is_protected_path() {
        let protected = vec!["Desktop".to_string(), "Downloads".to_string()];
        assert!(is_protected_path(
            &PathBuf::from("/home/user/Desktop/file.txt"),
            &protected
        ));
        assert!(is_protected_path(
            &PathBuf::from("/home/user/Downloads"),
            &protected
        ));
        assert!(!is_protected_path(&PathBuf::from("/tmp/cache"), &protected));
    }

    #[test]
    fn test_clean_result_empty() {
        let result = CleanResult::empty();
        assert_eq!(result.size, 0);
        assert_eq!(result.files_removed, 0);
        assert_eq!(result.dirs_removed, 0);
    }

    #[test]
    fn test_scan_directory() {
        let dir = setup_test_dir();
        let (size, count, files) = scan_directory(dir.path(), None);
        assert_eq!(count, 1);
        assert_eq!(size, 11);
        assert_eq!(files.len(), 1);
    }

    #[test]
    fn test_clean_directory_dry_run() {
        let dir = setup_test_dir();
        let protected = vec![];
        let result = clean_directory(dir.path(), true, &protected, None, false);
        assert_eq!(result.size, 11);
        assert_eq!(result.files_removed, 1);
        assert!(dir.path().join("test.txt").exists());
    }

    #[test]
    fn test_clean_directory_actual_remove() {
        let dir = setup_test_dir();
        let protected = vec![];
        let result = clean_directory(dir.path(), false, &protected, None, false);
        assert_eq!(result.size, 11);
        assert_eq!(result.files_removed, 1);
        assert!(!dir.path().join("test.txt").exists());
    }

    #[test]
    fn test_remove_lock_file_exists() {
        let dir = tempfile::tempdir().unwrap();
        let lock_path = dir.path().join("test.lck");
        File::create(&lock_path).unwrap();

        let result = remove_lock_file(&lock_path, true, false);
        assert_eq!(result.files_removed, 1);
        assert!(lock_path.exists());

        let result = remove_lock_file(&lock_path, false, false);
        assert_eq!(result.files_removed, 1);
        assert!(!lock_path.exists());
    }

    #[test]
    fn test_remove_lock_file_not_exists() {
        let dir = tempfile::tempdir().unwrap();
        let lock_path = dir.path().join("nonexistent.lck");
        let result = remove_lock_file(&lock_path, false, false);
        assert_eq!(result.size, 0);
    }

    #[test]
    fn test_parse_docker_size_gb() {
        assert_eq!(parse_docker_size("1.5GB"), 1_610_612_736);
        assert_ne!(parse_docker_size("2GB"), 0);
    }

    #[test]
    fn test_parse_docker_size_mb() {
        assert_eq!(parse_docker_size("500MB"), 524_288_000);
        assert_eq!(parse_docker_size("100MB"), 104_857_600);
    }

    #[test]
    fn test_parse_docker_size_kb() {
        assert_eq!(parse_docker_size("1024KB"), 1_048_576);
    }

    #[test]
    fn test_parse_docker_size_bytes() {
        assert_eq!(parse_docker_size("1024B"), 1024);
    }

    #[test]
    fn test_extract_pacman_package_name() {
        assert_eq!(
            extract_pacman_package_name("bash-5.2.0-1-x86_64.pkg.tar.zst"),
            "bash"
        );
        assert_eq!(
            extract_pacman_package_name("nodejs-20.0.0-1-x86_64.pkg.tar.xz"),
            "nodejs"
        );
    }

    #[test]
    fn test_clean_directory_skips_nonexistent() {
        let result = clean_directory(Path::new("/nonexistent/path"), true, &[], None, false);
        assert_eq!(result.size, 0);
        assert_eq!(result.files_removed, 0);
    }

    #[test]
    fn test_remove_file_exists() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("test.txt");
        File::create(&file_path).unwrap();

        let result = remove_file(&file_path, true, false);
        assert_eq!(result.files_removed, 1);
        assert!(file_path.exists());

        let result = remove_file(&file_path, false, false);
        assert_eq!(result.files_removed, 1);
        assert!(!file_path.exists());
    }

    #[test]
    fn test_remove_file_not_exists() {
        let result = remove_file(Path::new("/nonexistent/file"), false, false);
        assert_eq!(result.size, 0);
        assert_eq!(result.files_removed, 0);
    }
}
