// config.rs — YAML policy deserialisation
//
// The policy file is the single source of truth for the daemon's behaviour.
// Every field that has a sensible default is wrapped in Option<T> so the user
// only needs to write what they actually care about.

use serde::Deserialize;
use std::fs;

#[derive(Debug, Deserialize, Clone)]
pub struct Config {
    pub daemon: DaemonConfig,

    /// Filesystems to watch, each with independent threshold levels.
    pub filesystems: Vec<FilesystemPolicy>,

    /// Glob patterns (or literal paths) that lifecycle management must never
    /// delete or compress.  Commands specified in thresholds run as-is; the
    /// daemon does not intercept them.
    #[serde(default)]
    pub preserve: Vec<String>,

    /// Logrotate-style rules that run on a separate, slower schedule so the
    /// main polling loop stays responsive.
    #[serde(default)]
    pub lifecycle: Vec<LifecycleRule>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct DaemonConfig {
    /// How many seconds to sleep between disk-usage polls.
    /// Shorter = faster reaction; longer = less overhead.
    /// Default: 10
    pub interval_seconds: Option<u64>,

    /// A threshold must be exceeded for this many consecutive poll cycles
    /// before the daemon runs its reaction commands.  Prevents flapping.
    /// Default: 5
    pub health_window: Option<i32>,

    /// How often to run lifecycle rules (compress/delete), in seconds.
    /// Independent of the main polling interval so you can poll every 10 s
    /// but only churn through log files every hour.
    /// Default: equal to interval_seconds
    pub lifecycle_interval_seconds: Option<u64>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct FilesystemPolicy {
    /// Absolute path of the mount point (or any path on the target filesystem).
    /// statvfs is called on this path, so "/var" works even if it is not a
    /// separate mount — it will report figures for whichever filesystem owns it.
    pub mount: String,

    /// Ordered list of thresholds.  They are sorted ascending at load time so
    /// the YAML order does not matter, but ascending is conventional.
    pub thresholds: Vec<Threshold>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct Threshold {
    /// Integer percentage (0–100) at which this threshold is considered active.
    pub usage_percent: u32,

    /// Shell commands to run (via `sh -c`) when this threshold has been
    /// continuously active for `health_window` poll cycles.
    /// Each command is run once per trigger event, then suppressed until the
    /// filesystem recovers below this threshold and exceeds it again.
    #[serde(default)]
    pub commands: Vec<String>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct LifecycleRule {
    /// Glob pattern for the files this rule manages.
    /// Example: "/var/log/myapp/*.log"
    pub pattern: String,

    /// Delete uncompressed files older than this many days.
    pub max_age_days: Option<u64>,

    /// Truncate uncompressed files larger than this many mebibytes (1 MiB = 2²⁰ bytes).
    pub max_size_mb: Option<u64>,

    /// Compress files older than this many days (gzip, creates <file>.gz and
    /// removes the original).  Applied before max_age_days so you can set
    /// compress_after_days=7 and max_age_days=7 to ensure old originals are
    /// always compressed rather than deleted outright.
    pub compress_after_days: Option<u64>,

    /// Delete *.gz files (matched by appending ".gz" to the pattern) that are
    /// older than this many days.  Handled within the same rule for convenience.
    pub delete_compressed_after_days: Option<u64>,
}

impl Config {
    pub fn load(path: &str) -> Result<Config, Box<dyn std::error::Error>> {
        let raw = fs::read_to_string(path)
            .map_err(|e| format!("cannot read '{}': {}", path, e))?;

        let mut cfg: Config = serde_yaml::from_str(&raw)
            .map_err(|e| format!("YAML parse error in '{}': {}", path, e))?;

        for fs in &mut cfg.filesystems {
            fs.thresholds.sort_by_key(|t| t.usage_percent);
            for t in &fs.thresholds {
                if t.usage_percent > 100 {
                    return Err(format!(
                        "mount '{}': usage_percent {} is > 100",
                        fs.mount, t.usage_percent
                    )
                    .into());
                }
            }
        }

        Ok(cfg)
    }
}
