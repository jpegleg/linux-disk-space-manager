// disk.rs - filesystem usage measurement
//
// Uses POSIX statvfs (via the nix crate) so it works without /proc parsing on
// any Linux variant.  Returns a percentage in the range [0.0, 100.0].
// Both inode use and storage use are checked.

use nix::sys::statvfs::statvfs;

/// Returns the used-space percentage (0.0–100.0) for the filesystem that
/// `mount_path` lives on.  The path does not have to be a mount point - statvfs
/// reports figures for whichever filesystem owns the path.
///
/// Returns 0.0 on error (statvfs failure or zero-size filesystem) and prints
/// the error to stderr so the daemon can keep running.
///
/// Both inode percent and blocks percent are checked, using whichever value
/// is higher in the output. The default is block usage percent, but if
/// the inode usage is high, that will also count against the config
/// threshold. So each configured threshold applies to both inodes and blocks.
pub fn disk_usage_percent(mount_path: &str) -> f64 {
    match statvfs(mount_path) {
        Ok(stat) => {
            let total = stat.blocks() as u64;
            let free = stat.blocks_free() as u64;
            let avail = stat.blocks_available() as u64;
            let mut used = total.saturating_sub(free);
            let mut capacity = used + avail;

            let itotal = stat.files() as u64;
            let ifree = stat.files_free() as u64;
            let iavail = stat.files_available() as u64;
            let iused = itotal.saturating_sub(ifree);
            let icapacity = iused + iavail;

            if iused > used {
                used = iused;
                capacity = icapacity;
            };

            if capacity == 0 {
                return 0.0;
            }

            (used as f64 / capacity as f64) * 100.0
        }
        Err(e) => {
            eprintln!("ERROR: statvfs('{}') failed: {}", mount_path, e);
            0.0
        }
    }
}

pub fn disk_bytes(mount_path: &str) -> (u64, u64) {
    match statvfs(mount_path) {
        Ok(stat) => {
            let fsize = stat.fragment_size() as u64;
            let total = stat.blocks() as u64;
            let free = stat.blocks_free() as u64;
            let avail = stat.blocks_available() as u64;
            let mut used = total.saturating_sub(free);
            let mut capacity = used + avail;
            let itotal = stat.files() as u64;
            let ifree = stat.files_free() as u64;
            let iavail = stat.files_available() as u64;
            let iused = itotal.saturating_sub(ifree);
            let icapacity = iused + iavail;

            if iused > used {
                used = iused;
                capacity = icapacity;
            };

            (used * fsize, capacity * fsize)
        }
        Err(_) => (0, 0),
    }
}

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
