![cdlogo](https://carefuldata.com/images/cdlogo.png)

# linux disk space manager

This program is a controller daemon that runs on the underlying linux operating system, in virtual machines or baremetal.
There is a single YAML policy file that creates the rules for how disk space usage is responded to.
The YAML file also allows logrotate-like file lifecycle management.

It can also be run as any system user on any system slice, interactively or as a service/background task.
So a restricted user "bob" could still run the linux-disk-space-manager within "bob" unix permissions scope.

Linux-disk-space-manager can save systems from crashing because the log files filled up the disk slice, and has other administrative uses.

The way it is recommended to run is as an administrative user, so it can clean up any system slice.

The daemon runs as root typically so that it can manage the core of the system completely without restriction.
It is possible to run linux-disk-space-manager as a non-root user, it can be scoped to any user or system slice target.

Example policy YAML:

```
daemon:
  interval_seconds: 5
  health_window: 10
  lifecycle_interval_seconds: 3600
filesystems:
  - mount: /var
    thresholds:
      - usage_percent: 70
        commands:
          - "journalctl --vacuum-time=30d"
      - usage_percent: 85
        commands:
          - "journalctl --vacuum-time=15d"
          - "nice find /var/cache/apt/archives -name '*.deb' -mtime +14 -delete"
          - "docker system prune -af"
      - usage_percent: 92
        commands:
          - "journalctl --vacuum-time=1d"
          - "journalctl --vacuum-size=200M"
          - "apt-get clean -y || true"
          - "nice find /var/tmp -mindepth 1 -mtime +1 -delete"
      - usage_percent: 97
        commands:
          - "journalctl --vacuum-size=50M"
          - "rm -rf /var/tmp/*"
          - "sync && echo 3 > /proc/sys/vm/drop_caches"
  - mount: /
    thresholds:
      - usage_percent: 92
        commands:
          - "find /root/.cache -mindepth 1 -mtime +7 -delete"
      - usage_percent: 97
        commands:
          - "sync && echo 3 > /proc/sys/vm/drop_caches"
          - "nice find / -type f -name *.log -exec cp /dev/null {} \\;"
  - mount: /tmp
    thresholds:
      - usage_percent: 75
        commands:
          - "find /tmp -mindepth 1 -mtime +7 -delete"
      - usage_percent: 90
        commands:
          - "find /tmp -mindepth 1 -mtime +1 -delete"
      - usage_percent: 99
        commands:
          - "nice find /tmp -type f -exec rm -f {} \\ ;"

lifecycle:
  - pattern: /var/log/postgresql/*.gz
    max_age_days: 30
  - pattern: /var/log/clamav/*.gz
    max_age_days: 30
  - pattern: /var/log/crazy_app.log
    max_size_mb: 100 # truncate after file reaches 100 Mebibytes (1024 * 1024 bytes is a mebibyte) 

```

<b>
A common policy mistake is forgetting that the reactions will not respect a preserve rule: reactions are potentially destructive and can cause data loss, they run as root on the underlying system if the daemon is run as root, which is default.
</b>

The policy YAML is _everything_ for how effective the cleaning actually is, and is a sensitive file in terms of write access, since it likely is basically root command injection as a service.

Protect the YAML, `chmod 600 policy.yaml; chown root:root policy.yaml` and take special care about how the file is created, maintained, reviewed, tested, deployed, and so on.

Here is another example policy YAML that focuses on GitLab runner management and faster loops (faster response time):

```
daemon:
  interval_seconds: 1
  health_window: 9
  lifecycle_interval_seconds: 600

filesystems:

  ########################################
  # /var (highest risk: logs, cache, CI)
  ########################################
  - mount: /var
    thresholds:
      - usage_percent: 80
        commands:
          - "journalctl --vacuum-time=30d"

      - usage_percent: 85
        commands:
          - "journalctl --vacuum-time=14d"
          - "journalctl --vacuum-size=1G"
          - "apt-get clean -y || true"
          - "dnf clean all || yum clean all || true"
          - "zypper clean --all || true"
          - "nice find /var/log -type f -name '*.log' -size +200M -exec truncate -s 0 {} \\;"
          - "nice find /var/cache -type f -mtime +14 -delete"
          - "docker system prune -af --filter 'until=72h' || true"

      - usage_percent: 92
        commands:
          - "journalctl --vacuum-time=3d"
          - "journalctl --vacuum-size=500M"
          - "nice find /var/log -type f -name '*.gz' -mtime +14 -delete"
          - "nice find /var/tmp -mindepth 1 -mtime +2 -delete"
          - "nice find /var/lib/gitlab-runner/builds -mindepth 1 -mtime +1 -delete || true"
          - "nice find /var/lib/gitlab-runner/cache -mindepth 1 -mtime +3 -delete || true"
          - "docker system prune -af --volumes || true"

      - usage_percent: 97
        commands:
          - "journalctl --vacuum-size=100M"
          - "rm -rf /var/tmp/*"
          - "rm -rf /var/lib/gitlab-runner/builds/* || true"
          - "rm -rf /var/lib/gitlab-runner/cache/* || true"
          - "rm -rf /var/lib/docker/tmp/* || true"
          - "sync && echo 3 > /proc/sys/vm/drop_caches"

  ########################################
  # ROOT /
  ########################################
  - mount: /
    thresholds:
      - usage_percent: 92
        commands:
          - "nice find /root/.cache -mindepth 1 -mtime +7 -delete"
          - "nice find /root/ -type f -name '*.log' -mtime +90 -delete"

      - usage_percent: 97
        commands:
          - "nice find / -xdev -type f -name '*.log' -size +100M -exec truncate -s 0 {} \\;"
          - "sync && echo 3 > /proc/sys/vm/drop_caches"

  ########################################
  # /home (generally safe cleanup only)
  ########################################
  - mount: /home
    thresholds:
      - usage_percent: 85
        commands:
          - "nice find /home -type d -name '.cache' -mtime +30 -exec rm -rf {} +"

      - usage_percent: 92
        commands:
          - "nice find /home -type f -name '*.log' -mtime +30 -exec gzip -9 {} \\;"

  ########################################
  # /opt (opt app clean up)
  ########################################
  - mount: /opt
    thresholds:
      - usage_percent: 85
        commands:
          - "nice find /opt -type f -name '*.log' -size +100M -exec truncate -s 0 {} \\;"

      - usage_percent: 92
        commands:
          - "nice find /opt -type d -name 'old' -mtime +30 -exec rm -rf {} + || true"

  ########################################
  # /tmp (aggressive cleanup)
  ########################################
  - mount: /tmp
    thresholds:
      - usage_percent: 70
        commands:
          - "nice find /tmp -mindepth 1 -mtime +3 -delete"

      - usage_percent: 85
        commands:
          - "nice find /tmp -mindepth 1 -mtime +1 -delete"

      - usage_percent: 97
        commands:
          - "rm -rf /tmp/* || find /tmp/ -type f -exec rm -f {} \\;"

########################################
# LOG LIFECYCLE MANAGEMENT
########################################

lifecycle:

  # Core system logs (cross-distro)
  - pattern: /var/log/*.log
    max_age_days: 14

  - pattern: /var/log/*.gz
    max_age_days: 30

  # Debian / Ubuntu
  - pattern: /var/log/syslog.*
    max_age_days: 14

  - pattern: /var/log/auth.log.*
    max_age_days: 30

  - pattern: /var/log/dpkg.log.*
    max_age_days: 14

  # Fedora / RHEL / openSUSE
  - pattern: /var/log/messages.*
    max_age_days: 14

  - pattern: /var/log/secure.*
    max_age_days: 30

  - pattern: /var/log/dnf.log.*
    max_age_days: 14

  - pattern: /var/log/yum.log.*
    max_age_days: 14

  - pattern: /var/log/zypper.log.*
    max_age_days: 14

  # Kernel, cron, auth
  - pattern: /var/log/kern.log.*
    max_age_days: 14

  - pattern: /var/log/cron.*
    max_age_days: 14

  # Audit logs (keep longer)
  - pattern: /var/log/audit/*.log*
    max_age_days: 30

  # Web servers
  - pattern: /var/log/nginx/*.log*
    max_age_days: 14

  - pattern: /var/log/apache2/*.log*
    max_age_days: 14

  - pattern: /var/log/httpd/*.log*
    max_age_days: 14

  # Database logs
  - pattern: /var/log/postgresql/*.log*
    max_age_days: 30

  - pattern: /var/log/mysql/*.log*
    max_age_days: 30

  # Security / AV
  - pattern: /var/log/clamav/*.log*
    max_age_days: 30

  # GitLab Runner logs & artifacts
  - pattern: /var/lib/gitlab-runner/**/*.log
    max_age_days: 7

  - pattern: /var/lib/gitlab-runner/**/*.gz
    max_age_days: 14

```

Write your own policy YAML for the needs of the system running the daemon.

Use caution, and make sure that the actions are desired and in alignment with any required data retention policies or security policies.

The default policy location that the provided systemd unit file and packages use is /root/policy.yaml. This location and file name can be anything.
So if we want to run as non-root, we could have a policy in /home/bob/etc/bob-cleaner.yaml and have the service configured to run as bob and that path for the policy instead.

Both storage (blocks used/available) and inodes (files used/available) are used in the disk checks. The block usage is the default, meaning that `df` is similar to the standard
metric, however if inodes (similar to `df -i`) usage crosses a threshold, that metric will be used. This way if either storage percent or files percent exceed config thresholds,
the actions will be triggered. There are not separate blocks vs inode thresholds.

## installing

Installation methods being worked on include:

```
crates.io: cargo install linux-disk-space-manager

...

compile from github source: cargo build --release && mv target/release/linux-disk-space-manager /usr/local/sbin/

...

install precompiled binary

...

use release debian package: sudo dpkg -i linux-disk-space-manager.deb

```

There is a provided example systemd unit file that is also used in the debian package example. To manually install the systemd unit file:

```
sudo cp linux-disk-space-manager.service /etc/systemd/system/
sudo systemctl enable linux-disk-space-manager
sudo systemctl start linux-disk-space-manager
```

To manually install a policy, here is an example approach:

```
sudo cp policy.yaml /root/policy.yaml
sudo chown root:root /root/policy.yaml
sudo chmod 600 /root/policy.yaml
```

## logging

The default log level the daemon typically uses is `warn` (-w):

```
[2026-04-26T01:55:28Z INFO ] - linux disk space manager - "db45prod" - linux-disk-space-manager v1.0.7 started  policy=policy.yaml  interval=2s  health_window=10  lifecycle_interval=60s
[2026-04-26T01:55:28Z INFO ] - linux disk space manager - "db45prod" - watching '/var' — 3.3 GiB/21.1 GiB used (15.5%), 4 threshold(s)
[2026-04-26T01:55:28Z INFO ] - linux disk space manager - "db45prod" - watching '/' — 9.6 GiB/54.8 GiB used (17.5%), 2 threshold(s)
[2026-04-26T01:55:28Z INFO ] - linux disk space manager - "db45prod" - watching '/tmp' — 159.8 MiB/2.7 GiB used (5.8%), 3 threshold(s)
[2026-04-26T01:55:28Z WARN ] - linux disk space manager - "db45prod" - lifecycle: truncating (312 MiB >= 10 MiB): /var/log/turkey.log
[2026-04-26T01:56:28Z WARN ] - linux disk space manager - "db45prod" - lifecycle: truncating (938 MiB >= 10 MiB): /var/log/turkey.log
[2026-04-26T01:57:16Z WARN ] - linux disk space manager - "db45prod" - [/tmp] 76.2% — 75% threshold reached (cycle 1/10)
[2026-04-26T01:57:20Z WARN ] - linux disk space manager - "db45prod" - [/tmp] 96.8% — 90% threshold reached (cycle 1/10)
[2026-04-26T01:57:22Z WARN ] - linux disk space manager - "db45prod" - [/tmp] 100.0% — 99% threshold reached (cycle 1/10)
[2026-04-26T01:57:34Z WARN ] - linux disk space manager - "db45prod" - [/tmp] 100.0% >= 75% sustained for 10 cycles — spawning 1 reaction thread
[2026-04-26T01:57:34Z INFO ] - linux disk space manager - "db45prod" - running reaction: find /tmp -mindepth 1 -mtime +7 -delete
[2026-04-26T01:57:38Z WARN ] - linux disk space manager - "db45prod" - [/tmp] 100.0% >= 90% sustained for 10 cycles — spawning 1 reaction thread
[2026-04-26T01:57:38Z INFO ] - linux disk space manager - "db45prod" - running reaction: find /tmp -mindepth 1 -mtime +1 -delete
[2026-04-26T01:57:40Z WARN ] - linux disk space manager - "db45prod" - [/tmp] 100.0% >= 99% sustained for 10 cycles — spawning 1 reaction thread
[2026-04-26T01:57:40Z INFO ] - linux disk space manager - "db45prod" - running reaction: rm -rf /tmp/*
[2026-04-26T01:57:42Z INFO ] - linux disk space manager - "db45prod" - [/tmp] recovered below 75% — now at 5.8% (159.8 MiB/2.7 GiB)
[2026-04-26T01:57:42Z INFO ] - linux disk space manager - "db45prod" - [/tmp] recovered below 90% — now at 5.8% (159.8 MiB/2.7 GiB)
[2026-04-26T01:57:42Z INFO ] - linux disk space manager - "db45prod" - [/tmp] recovered below 99% — now at 5.8% (159.8 MiB/2.7 GiB)

```

There is also quiet mode (-q) and debug mode (-d). Quiet mode is ueful for systems that are sensitive for IO or systems where the daemon is very active
and we don't want the linux-disk-space-manager logs to contribute to disk space issues themselves!

## example of running in the foreground in debug mode

While the program is designed to be a daemon and run in the background, the program can be run manually in the foreground of a terminal session instead.

This is especially useful for test and QA systems, testing the policy YAML and making sure the handling and data retention are as desired.

Here is an example of running the linux-disk-space-manager manually in debug mode:

```
$ linux-disk-space-manager ./policy.yaml -d 2>&1 | tee disk_manager_$(date +%Y%m%d%H%M%S).log
[2026-04-26T02:01:47Z INFO ] - linux disk space manager - "db45prod" - linux-disk-space-manager v1.0.7 started  policy=policy.yaml  interval=2s  health_window=10  lifecycle_interval=60s
[2026-04-26T02:01:47Z INFO ] - linux disk space manager - "db45prod" - watching '/var' — 3.0 GiB/21.1 GiB used (14.1%), 4 threshold(s)
[2026-04-26T02:01:47Z INFO ] - linux disk space manager - "db45prod" - watching '/' — 9.6 GiB/54.8 GiB used (17.5%), 2 threshold(s)
[2026-04-26T02:01:47Z INFO ] - linux disk space manager - "db45prod" - watching '/tmp' — 159.8 MiB/2.7 GiB used (5.8%), 3 threshold(s)
[2026-04-26T02:01:47Z DEBUG] - linux disk space manager - "db45prod" - lifecycle: spawning background thread
[2026-04-26T02:01:47Z DEBUG] - linux disk space manager - "db45prod" - [/var] 14.1% used (3.0 GiB/21.1 GiB)
[2026-04-26T02:01:47Z DEBUG] - linux disk space manager - "db45prod" - [/] 17.5% used (9.6 GiB/54.8 GiB)
[2026-04-26T02:01:47Z DEBUG] - linux disk space manager - "db45prod" - [/tmp] 5.8% used (159.8 MiB/2.7 GiB)
[2026-04-26T02:01:47Z DEBUG] - linux disk space manager - "db45prod" - cycle completed 0ms — sleeping 1999ms
[2026-04-26T02:01:47Z DEBUG] - linux disk space manager - "db45prod" - lifecycle: running 7 rule(s)
[2026-04-26T02:01:47Z DEBUG] - linux disk space manager - "db45prod" - lifecycle: ok [11 days, 0 MiB] /var/log/postgresql/postgresql-17-main.log.2.gz
[2026-04-26T02:01:47Z DEBUG] - linux disk space manager - "db45prod" - lifecycle: ok [21 days, 0 MiB] /var/log/postgresql/postgresql-17-main.log.3.gz
[2026-04-26T02:01:47Z DEBUG] - linux disk space manager - "db45prod" - lifecycle: ok [0 days, 0 MiB] /var/log/boot.log.1
[2026-04-26T02:01:47Z DEBUG] - linux disk space manager - "db45prod" - lifecycle: ok [0 days, 0 MiB] /var/log/boot.log.2.gz
[2026-04-26T02:01:47Z DEBUG] - linux disk space manager - "db45prod" - lifecycle: ok [0 days, 0 MiB] /var/log/boot.log.3.gz
[2026-04-26T02:01:47Z DEBUG] - linux disk space manager - "db45prod" - lifecycle: ok [0 days, 0 MiB] /var/log/boot.log.4.gz
[2026-04-26T02:01:47Z DEBUG] - linux disk space manager - "db45prod" - lifecycle: ok [0 days, 0 MiB] /var/log/boot.log.5.gz
[2026-04-26T02:01:47Z DEBUG] - linux disk space manager - "db45prod" - lifecycle: ok [0 days, 0 MiB] /var/log/boot.log.6.gz
[2026-04-26T02:01:47Z DEBUG] - linux disk space manager - "db45prod" - lifecycle: ok [0 days, 0 MiB] /var/log/boot.log.7.gz
[2026-04-26T02:01:47Z DEBUG] - linux disk space manager - "db45prod" - lifecycle: ok [0 days, 0 MiB] /var/log/turkey.log
[2026-04-26T02:01:49Z DEBUG] - linux disk space manager - "db45prod" - [/var] 14.1% used (3.0 GiB/21.1 GiB)
[2026-04-26T02:01:49Z DEBUG] - linux disk space manager - "db45prod" - [/] 17.5% used (9.6 GiB/54.8 GiB)
[2026-04-26T02:01:49Z DEBUG] - linux disk space manager - "db45prod" - [/tmp] 5.8% used (159.8 MiB/2.7 GiB)
[2026-04-26T02:01:49Z DEBUG] - linux disk space manager - "db45prod" - cycle completed 0ms — sleeping 1999ms
[2026-04-26T02:01:51Z DEBUG] - linux disk space manager - "db45prod" - [/var] 14.1% used (3.0 GiB/21.1 GiB)
[2026-04-26T02:01:51Z DEBUG] - linux disk space manager - "db45prod" - [/] 17.5% used (9.6 GiB/54.8 GiB)
[2026-04-26T02:01:51Z DEBUG] - linux disk space manager - "db45prod" - [/tmp] 5.8% used (159.8 MiB/2.7 GiB)
[2026-04-26T02:01:51Z DEBUG] - linux disk space manager - "db45prod" - cycle completed 0ms — sleeping 1999ms
[2026-04-26T02:01:53Z DEBUG] - linux disk space manager - "db45prod" - [/var] 14.1% used (3.0 GiB/21.1 GiB)
[2026-04-26T02:01:53Z DEBUG] - linux disk space manager - "db45prod" - [/] 17.5% used (9.6 GiB/54.8 GiB)
[2026-04-26T02:01:53Z DEBUG] - linux disk space manager - "db45prod" - [/tmp] 5.8% used (159.8 MiB/2.7 GiB)
[2026-04-26T02:01:53Z DEBUG] - linux disk space manager - "db45prod" - cycle completed 0ms — sleeping 1999ms
[2026-04-26T02:01:55Z DEBUG] - linux disk space manager - "db45prod" - [/var] 14.1% used (3.0 GiB/21.1 GiB)
[2026-04-26T02:01:55Z DEBUG] - linux disk space manager - "db45prod" - [/] 17.5% used (9.6 GiB/54.8 GiB)
[2026-04-26T02:01:55Z DEBUG] - linux disk space manager - "db45prod" - [/tmp] 5.8% used (159.8 MiB/2.7 GiB)
[2026-04-26T02:01:55Z DEBUG] - linux disk space manager - "db45prod" - cycle completed 0ms — sleeping 1999ms
...
```

We might run linux-disk-space-manager interactively instead of as a daemon. Here is an example of a user running it within their own `/home/` directory scope.


Example user bob's policy:

```
daemon:
  interval_seconds: 1
  health_window: 3
  lifecycle_interval_seconds: 600
filesystems:
  - mount: /home/bob/
    thresholds:
      - usage_percent: 70
        commands:
          - "nice find /home/bob/tmp -mindepth 1 -mtime +3 -delete"

      - usage_percent: 85
        commands:
          - "nice find /home/bob/tmp -mindepth 1 -mtime +1 -delete"

      - usage_percent: 97
        commands:
          - "rm -rf /home/bob/tmp/* /home/bob/logs/backups/* || find /home/bob/tmp -type f -exec rm -f {} \\;"

lifecycle:
  - pattern: /home/bob/log/**/*.gz
    max_age_days: 14

```


Note that the system slice is commonly `/home` not `/home/bob`, so the metrics in bob's case are actually from the `/home` slice,
so another user could fill up that slice, and non of bob's reactions would solve such a condition.

But if each user has their own linux-disk-space-manager scope and policy, then all of `/home` is covered generally.

And bob running his policiy in the foreground, such as in a tmux session.

```
 λ linux-disk-space-manager policy.yaml -d
[2026-06-07T22:09:41Z INFO ] - linux disk space manager - "myPC" - linux-disk-space-manager v1.0.5 started  policy=policy.yaml  interval=1s  health_window=3  lifecycle_interval=600s
[2026-06-07T22:09:41Z INFO ] - linux disk space manager - "myPC" - watching '/home/bob/' — 64.8 GiB/784.9 GiB used (8.3%), 3 threshold(s)
[2026-06-07T22:09:41Z DEBUG] - linux disk space manager - "myPC" - lifecycle: spawning background thread
[2026-06-07T22:09:41Z DEBUG] - linux disk space manager - "myPC" - [/home/bob/] 8.3% used (64.8 GiB/784.9 GiB)
[2026-06-07T22:09:41Z DEBUG] - linux disk space manager - "myPC" - cycle completed 0ms — sleeping 999ms
[2026-06-07T22:09:41Z DEBUG] - linux disk space manager - "myPC" - lifecycle: running 1 rule(s)
[2026-06-07T22:09:41Z DEBUG] - linux disk space manager - "myPC" - lifecycle: ok [0 days, 0 MiB] /home/bob/log/hunter/ps.log.gz
[2026-06-07T22:09:42Z DEBUG] - linux disk space manager - "myPC" - [/home/bob/] 8.3% used (64.8 GiB/784.9 GiB)
[2026-06-07T22:09:42Z DEBUG] - linux disk space manager - "myPC" - cycle completed 0ms — sleeping 999ms
[2026-06-07T22:09:43Z DEBUG] - linux disk space manager - "myPC" - [/home/bob/] 8.3% used (64.8 GiB/784.9 GiB)
[2026-06-07T22:09:43Z DEBUG] - linux disk space manager - "myPC" - cycle completed 0ms — sleeping 999ms
[2026-06-07T22:09:44Z DEBUG] - linux disk space manager - "myPC" - [/home/bob/] 8.3% used (64.8 GiB/784.9 GiB)
[2026-06-07T22:09:44Z DEBUG] - linux disk space manager - "myPC" - cycle completed 0ms — sleeping 999ms
[2026-06-07T22:09:45Z DEBUG] - linux disk space manager - "myPC" - [/home/bob/] 8.3% used (64.8 GiB/784.9 GiB)
[2026-06-07T22:09:45Z DEBUG] - linux disk space manager - "myPC" - cycle completed 0ms — sleeping 999ms

```


Rather than having many copies and individual scopes, having a singular instance as root is more simple, easier to manager,
reduces load on the system, and is less likely to encounter limits because of permissions or login groups, etc. One of 
the reasons not to do that, is to separate security concerns. Individual developers may be more aware of the log files their
tests create, so they may be the ones who should write policy. This is when individual user instances show more value.

### operating systems

This program is designed for linux systems with rapidly filling application logs, but can be run on other operating systems.

OpenBSD 7.9 is working fine, tested on June 7th, 2026.

All testing for releases are done on Debian Linux. Most unix-like systems should be able to get it working, via nix.

The OS level upstream dependency for disk metrics is [nix](https://crates.io/crates/nix), which provides
the interface to gether the disk usage information. If "nix" works, then linux-disk-space-manager likely works.
It uses the `statvfs.blocks_free()`, and as of v1.0.8, also `statvfs.files_free()` to collect the required disk information.

The command execution for reactions is built via `Command::new("sh")`, so any system with `sh` should work
for the command execution. There is no intent on supporting other command execution binaries or options.

Some amount of effort has been put in to shrink the binary size by default. This is to minimize the footprint
on the underlying system. With binaries that are 700KB or so, we can include the software on extremely small
systems. Update the Cargo.toml release section to adjust this if you want to optimize for debugging etc.

# change log and version use

1.0.9 - documentation and logging improvement updates

1.0.8 - add inode calculation to the usage metrics, increase max wait period between reminder log lines

1.0.7 - fix version printing in the logs, align to release version


1.0.6 - fix disk calculation approach to more closely match 'df -h' output
        (this version prints as 1.0.5 in the logs...)

1.0.4 and 1.0.5 - fixes important mistake in disk calculation

1.0.3 and older - don't use these
