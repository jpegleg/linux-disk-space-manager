// disk.rs — filesystem usage measurement
//
// Uses POSIX statvfs (via the nix crate) so it works without /proc parsing on
// any Linux variant.  Returns a percentage in the range [0.0, 100.0].

use nix::sys::statvfs::statvfs;

/// Returns the used-space percentage (0.0–100.0) for the filesystem that
/// `mount_path` lives on.  The path does not have to be a mount point — statvfs
/// reports figures for whichever filesystem owns the path.
///
/// Returns 0.0 on error (statvfs failure or zero-size filesystem) and prints
/// the error to stderr so the daemon can keep running.
pub fn disk_usage_percent(mount_path: &str) -> f64 {
    match statvfs(mount_path) {
        Ok(stat) => {
            let fsize = stat.fragment_size();
            let total = stat.blocks();
            let avail = stat.blocks_available();
            if total == 0 {
                return 0.0;
            }
            let used = total.saturating_sub(avail);
            (used as f64 / total as f64) * 100.0
        }
        Err(e) => {
            eprintln!("ERROR: statvfs('{}') failed: {}", mount_path, e);
            0.0
        }
    }
}

/// Returns (used_bytes, total_bytes) for human-readable log lines.
pub fn disk_bytes(mount_path: &str) -> (u64, u64) {
    match statvfs(mount_path) {
        Ok(stat) => {
            let fsize = stat.fragment_size();
            let total = stat.blocks();
            let avail = stat.blocks_available();
            let used = total.saturating_sub(avail);
            (used, total)
        }
        Err(_) => (0, 0),
    }
}

/// Format bytes as a human-readable string (GiB / MiB / KiB / B).
pub fn human_bytes(bytes: u64) -> String {
    const GIB: u64 = 1 << 30;
    const MIB: u64 = 1 << 20;
    const KIB: u64 = 1 << 10;
    if bytes >= GIB {
        format!("{:.1} GiB", bytes as f64 / GIB as f64)
    } else if bytes >= MIB {
        format!("{:.1} MiB", bytes as f64 / MIB as f64)
    } else if bytes >= KIB {
        format!("{:.1} KiB", bytes as f64 / KIB as f64)
    } else {
        format!("{} B", bytes)
    }
}
