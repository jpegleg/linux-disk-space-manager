use std::{env, thread, time};
use gethostname::gethostname;

mod config;
mod disk;
mod lifecycle;
mod logger;
mod runner;

use config::Config;
use disk::{disk_bytes, disk_usage_percent, human_bytes};
use lifecycle::apply_lifecycle;
use logger::{Level, Logger};
use runner::run_commands;

struct ThresholdState {
    active_cycles: i32,
    triggered: bool,
    handle: Option<thread::JoinHandle<()>>,
}

impl Default for ThresholdState {
    fn default() -> Self {
        ThresholdState {
            active_cycles: 0,
            triggered: false,
            handle: None,
        }
    }
}

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() == 2 && (args[1] == "--help" || args[1] == "-h") {
        print_help(&args[0]);
        return;
    }

    if args.len() != 3 {
        eprintln!("Usage: {} <policy.yaml> [-d|-w|-q]", args[0]);
        std::process::exit(1);
    }

    let policy_path = &args[1];

    let level = match Level::from_flag(&args[2]) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("ERROR: {}", e);
            std::process::exit(1);
        }
    };

    let config = match Config::load(policy_path) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("ERROR: {}", e);
            std::process::exit(1);
        }
    };

    let log = Logger::new(gethostname(), level);
    let interval_secs = config.daemon.interval_seconds.unwrap_or(10).max(1);
    let health_window = config.daemon.health_window.unwrap_or(5).max(1);
    let lifecycle_interval_secs = config
        .daemon
        .lifecycle_interval_seconds
        .unwrap_or(interval_secs);

    log.info(&format!(
        "linux-disk-space-manager v1.0.3 started  policy={}  interval={}s  health_window={}  lifecycle_interval={}s",
        policy_path, interval_secs, health_window, lifecycle_interval_secs
    ));

    for fs in &config.filesystems {
        let (used, total) = disk_bytes(&fs.mount);
        if total == 0 {
            log.warn(&format!("startup: cannot read '{}' — check path or permissions", fs.mount));
        } else {
            log.info(&format!(
                "watching '{}' — {}/{} used ({:.1}%), {} threshold(s)",
                fs.mount,
                human_bytes(used),
                human_bytes(total),
                disk_usage_percent(&fs.mount),
                fs.thresholds.len()
            ));
        }
    }

    let mut states: Vec<Vec<ThresholdState>> = config
        .filesystems
        .iter()
        .map(|fs| fs.thresholds.iter().map(|_| ThresholdState::default()).collect())
        .collect();

    let preserve = config.preserve.clone();
    let lifecycle_rules = config.lifecycle.clone();
    let mut last_lifecycle_run: Option<time::Instant> = None;
    let mut lifecycle_handle: Option<thread::JoinHandle<()>> = None;

    loop {
        let cycle_start = time::Instant::now();

        if !lifecycle_rules.is_empty() {
            let due = last_lifecycle_run
                .map(|t| t.elapsed().as_secs() >= lifecycle_interval_secs)
                .unwrap_or(true);

            let prev_done = lifecycle_handle
                .as_ref()
                .map(|h| h.is_finished())
                .unwrap_or(true);

            if due && prev_done {
                let rules = lifecycle_rules.clone();
                let pres = preserve.clone();
                let llog = log.clone();
                log.debug("lifecycle: spawning background thread");
                lifecycle_handle = Some(thread::spawn(move || {
                    apply_lifecycle(&rules, &pres, &llog);
                }));
                last_lifecycle_run = Some(time::Instant::now());
            } else if due && !prev_done {
                log.debug("lifecycle: previous pass still running, skipping this interval");
            }
        }

        for (i, fs) in config.filesystems.iter().enumerate() {
            let usage_pct = disk_usage_percent(&fs.mount);
            let (used, total) = disk_bytes(&fs.mount);

            log.debug(&format!(
                "[{}] {:.1}% used ({}/{})",
                fs.mount,
                usage_pct,
                human_bytes(used),
                human_bytes(total)
            ));

            for (j, threshold) in fs.thresholds.iter().enumerate() {
                let state = &mut states[i][j];

                if usage_pct >= threshold.usage_percent as f64 {
                    state.active_cycles = (state.active_cycles + 1).min(i32::MAX - 1);

                    if state.active_cycles == 1 {
                        log.warn(&format!(
                            "[{}] {:.1}% — {}% threshold reached (cycle 1/{})",
                            fs.mount, usage_pct, threshold.usage_percent, health_window
                        ));
                    }

                    if state.active_cycles >= health_window && !state.triggered {
                        log.warn(&format!(
                            "[{}] {:.1}% >= {}% sustained for {} cycles — spawning {} reaction thread",
                            fs.mount,
                            usage_pct,
                            threshold.usage_percent,
                            state.active_cycles,
                            threshold.commands.len()
                        ));

                        let cmds = threshold.commands.clone();
                        let tlog = log.clone();
                        let handle = thread::spawn(move || {
                            run_commands(&cmds, &tlog);
                        });

                        state.handle = Some(handle);
                        state.triggered = true;
                    }

                    let reminder_period = (health_window * 10).max(20) as i32;
                    if state.triggered && state.active_cycles % reminder_period == 0 {
                        log.warn(&format!(
                            "[{}] disk still at {:.1}% >= {}% — {} cycles since reactions fired",
                            fs.mount, usage_pct, threshold.usage_percent, state.active_cycles
                        ));
                    }
                } else {
                    if state.active_cycles > 0 || state.triggered {
                        let was_triggered = state.triggered;
                        state.active_cycles = 0;
                        state.triggered = false;
                        state.handle = None;

                        if was_triggered {
                            log.info(&format!(
                                "[{}] recovered below {}% — now at {:.1}% ({}/{})",
                                fs.mount,
                                threshold.usage_percent,
                                usage_pct,
                                human_bytes(used),
                                human_bytes(total)
                            ));
                        }
                    }
                }
            }
        }

        let elapsed = cycle_start.elapsed();
        let sleep_for = time::Duration::from_secs(interval_secs).saturating_sub(elapsed);
        log.debug(&format!(
            "cycle completed {:.0}ms — sleeping {:.0}ms",
            elapsed.as_millis(),
            sleep_for.as_millis()
        ));
        thread::sleep(sleep_for);
    }
}

fn print_help(prog: &str) {
    println!(
        r#"linux-disk-space-manager — Linux disk-space management daemon

USAGE:
    {prog} <policy.yaml> [-d|-w|-q]

ARGUMENTS:
    <policy.yaml>   Path to YAML policy file (see below)
    -d              Debug logging — per-cycle disk stats, command output
    -w              Warn  logging — threshold events and reactions only
    -q              Quiet         — errors to stderr only

POLICY FILE STRUCTURE:

    daemon:
      interval_seconds: 10         # poll every 10 s (default)
      health_window: 5             # cycles before firing reactions (default)
      lifecycle_interval_seconds: 3600   # run lifecycle rules hourly

    filesystems:
      - mount: /
        thresholds:
          - usage_percent: 70
            commands:
              - "journalctl --vacuum-time=30d"
          - usage_percent: 85
            commands:
              - "journalctl --vacuum-time=15d"
          - usage_percent: 90
            commands:
              - "journalctl --vacuum-time=1d"
              - "apt-get clean -y"

    preserve:
      - /var/log/critical-app.log
      - /var/log/audit/*.log         # glob patterns supported

    lifecycle:
      - pattern: /var/log/myapp/*.log
        compress_after_days: 7
        delete_compressed_after_days: 90
        max_age_days: 7
        max_size_mb: 512

NOTES:
    • Thresholds are independent — each has its own health counter and
      triggered state.  Multiple levels can be active simultaneously.
    • A threshold fires its commands exactly once per breach event.
      It resets when usage drops below the threshold percent.
    • 'preserve' patterns prevent lifecycle management from touching matched
      files.  They do NOT intercept commands you run via threshold reactions.
    • Commands are run via 'sh -c', so pipes, redirects, and shell builtins
      all work.  Failures are logged but do not stop other commands.
"#,
        prog = prog
    );
}
