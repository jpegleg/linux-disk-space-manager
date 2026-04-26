// lifecycle.rs — logrotate-style file lifecycle management
//
// Runs on a configurable schedule (lifecycle_interval_seconds) that is
// typically much slower than the main polling loop so it does not add overhead
// to every disk-usage check.
//
// Rule application order per file (each step can short-circuit):
//   1. Skip if path matches a preserve pattern.
//   2. compress_after_days  → gzip the file if old enough (and not already .gz).
//   3. delete_compressed_after_days → delete *.gz siblings if old enough.
//   4. max_age_days  → delete uncompressed file if old enough.
//   5. max_size_mb   → delete uncompressed file if too large.
//
// The preserve list applies ONLY to lifecycle-driven deletes/compresses.
// Commands executed by threshold reactions run as-is and are not intercepted.

use std::fs;
use std::io::{self, BufReader};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use flate2::write::GzEncoder;
use flate2::Compression;
use glob::glob;

use crate::config::LifecycleRule;
use crate::logger::Logger;

pub fn apply_lifecycle(rules: &[LifecycleRule], preserve: &[String], log: &Logger) {
    log.debug(&format!("lifecycle: running {} rule(s)", rules.len()));
    for rule in rules {
        apply_rule(rule, preserve, log);
    }
}

fn apply_rule(rule: &LifecycleRule, preserve: &[String], log: &Logger) {
    let paths = match glob(&rule.pattern) {
        Ok(p)  => p,
        Err(e) => {
            log.error(&format!("lifecycle: invalid glob '{}': {}", rule.pattern, e));
            return;
        }
    };

    let gz_pattern = format!("{}.gz", &rule.pattern);
    let gz_paths: Vec<PathBuf> = rule
        .delete_compressed_after_days
        .map(|_| {
            glob(&gz_pattern)
                .ok()
                .map(|iter| iter.flatten().collect::<Vec<_>>())
                .unwrap_or_default()
        })
        .unwrap_or_default();

    if let Some(max_gz_days) = rule.delete_compressed_after_days {
        for gz_path in gz_paths {
            if is_preserved(&gz_path, preserve, log) {
                continue;
            }
            match file_age_days(&gz_path) {
                Some(age) if age >= max_gz_days => {
                    log.warn(&format!(
                        "lifecycle: deleting compressed file ({} days old, max {}): {}",
                        age,
                        max_gz_days,
                        gz_path.display()
                    ));
                    remove_file(&gz_path, log);
                }
                _ => {}
            }
        }
    }

    for entry in paths.flatten() {
        if !entry.is_file() {
            continue;
        }
        if is_preserved(&entry, preserve, log) {
            continue;
        }

        let age_days   = file_age_days(&entry);
        let size_mb    = fs::metadata(&entry).map(|m| m.len() / (1024 * 1024)).ok();

        // Step 1: compress_after_days — compress and move on; the .gz will be
        //         handled by delete_compressed_after_days in a future run.
        if let (Some(compress_after), Some(age)) = (rule.compress_after_days, age_days) {
            if age >= compress_after {
                log.warn(&format!(
                    "lifecycle: compressing ({} days old, threshold {}): {}",
                    age,
                    compress_after,
                    entry.display()
                ));
                compress_file(&entry, log);
                continue;
            }
        }

        // Step 2: max_age_days
        if let (Some(max_age), Some(age)) = (rule.max_age_days, age_days) {
            if age >= max_age {
                log.warn(&format!(
                    "lifecycle: deleting ({} days old, max {}): {}",
                    age,
                    max_age,
                    entry.display()
                ));
                remove_file(&entry, log);
                continue;
            }
        }

        // Step 3: max_size_mb
        if let (Some(max_mb), Some(sz_mb)) = (rule.max_size_mb, size_mb) {
            if sz_mb >= max_mb {
                log.warn(&format!(
                    "lifecycle: deleting ({} MiB, max {}): {}",
                    sz_mb,
                    max_mb,
                    entry.display()
                ));
                remove_file(&entry, log);
                continue;
            }
        }

        // No action needed — log at debug so operators can see the decision.
        log.debug(&format!(
            "lifecycle: ok [{} days, {} MiB] {}",
            age_days.map(|d| d.to_string()).unwrap_or_else(|| "?".into()),
            size_mb.map(|s| s.to_string()).unwrap_or_else(|| "?".into()),
            entry.display()
        ));
    }
}

/// Returns the file's age in whole days based on mtime, or None on error.
fn file_age_days(path: &Path) -> Option<u64> {
    let meta     = fs::metadata(path).ok()?;
    let modified = meta.modified().ok()?;
    let elapsed  = SystemTime::now().duration_since(modified).ok()?;
    Some(elapsed.as_secs() / 86_400)
}

/// Returns true if `path` matches any entry in the preserve list.
/// Preserve entries may be globs or literal paths.
fn is_preserved(path: &Path, preserve: &[String], log: &Logger) -> bool {
    let path_str = path.to_string_lossy();

    for pattern in preserve {
        match glob(pattern) {
            Ok(iter) => {
                for preserved in iter.flatten() {
                    if preserved == path {
                        log.debug(&format!(
                            "lifecycle: preserving '{}' (matched '{}')",
                            path_str, pattern
                        ));
                        return true;
                    }
                }
            }
            Err(_) => {}
        }
        // Also compare as a literal string for non-glob preserve entries.
        if path_str == pattern.as_str() {
            log.debug(&format!("lifecycle: preserving '{}'", path_str));
            return true;
        }
    }
    false
}

fn compress_file(path: &Path, log: &Logger) {
    let gz_path = {
        let mut p = path.to_path_buf();
        let mut name = p
            .file_name()
            .unwrap_or_default()
            .to_os_string();
        name.push(".gz");
        p.set_file_name(name);
        p
    };

    let src = match fs::File::open(path) {
        Ok(f)  => f,
        Err(e) => {
            log.error(&format!(
                "lifecycle: cannot open '{}' for compression: {}",
                path.display(), e
            ));
            return;
        }
    };

    let dst = match fs::File::create(&gz_path) {
        Ok(f)  => f,
        Err(e) => {
            log.error(&format!(
                "lifecycle: cannot create '{}': {}",
                gz_path.display(), e
            ));
            return;
        }
    };

    let mut encoder = GzEncoder::new(dst, Compression::default());
    let copy_result = io::copy(&mut BufReader::new(src), &mut encoder);

    match copy_result {
        Err(e) => {
            log.error(&format!(
                "lifecycle: compression failed for '{}': {}",
                path.display(), e
            ));
            let _ = fs::remove_file(&gz_path);
            return;
        }
        Ok(_) => {}
    }

    if let Err(e) = encoder.finish() {
        log.error(&format!(
            "lifecycle: gzip finalize failed for '{}': {}",
            gz_path.display(), e
        ));
        let _ = fs::remove_file(&gz_path);
        return;
    }

    if let Err(e) = fs::remove_file(path) {
        log.error(&format!(
            "lifecycle: could not remove original '{}' after compression: {}",
            path.display(), e
        ));
    } else {
        log.debug(&format!(
            "lifecycle: compressed '{}' → '{}'",
            path.display(),
            gz_path.display()
        ));
    }
}

fn remove_file(path: &Path, log: &Logger) {
    if let Err(e) = fs::remove_file(path) {
        log.error(&format!(
            "lifecycle: failed to delete '{}': {}",
            path.display(), e
        ));
    } else {
        log.debug(&format!("lifecycle: deleted '{}'", path.display()));
    }
}
