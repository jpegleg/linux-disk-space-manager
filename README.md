![cdlogo](https://carefuldata.com/images/cdlogo.png)

# linux disk space manager

This program is a controller daemon that runs on the underlying linux operating sytsem, in virtual machines or baremetal.
There is a single YAML policy file that creates the rules for how disk space usage is responded to. 
The YAML file also allows logrotate-like file lifecycle management.

The daemon runs as root typically, so that it can manage the core of the system completely without restriction.

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
          - "nice find / -type f -name *.log -exec cp /dev/null {} \ ; &"
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
          - "rm -rf /tmp/*"

lifecycle:
  - pattern: /var/log/postgresql/*.gz
    delete_compressed_after_days: 90
    max_age_days: 90
  - pattern: /var/crash/*.core
    max_age_days: 7

```

<b>
A common policy mistake is forgetting that the reactions will not respect a preserve rule: reactions are potentially destructive and can cause data loss, they run as root on the underlying system if the daemon is run as root, which is default.
</b>

The policy YAML is _everything_, and is a sensitive file in terms of write access, since it is basically root command injection as a service. 

Protect the YAML, `chmod 600 policy.yaml; chown root:root policy.yaml` and take special care about how the file is created, maintained, reviewed, tested, deployed, and so on.

## installing

Installation methods being worked on include:

```
crates.io

compile from github source

install precompiled binary

use debian package

```

More information about installation will be added to this section.

## logging


The default log level the daemon typically uses is `warn` (-w):

```
[2026-04-25T21:26:41Z INFO ] - linux disk space manager - "db45prod" - linux-disk-space-manager started  policy=policy.yaml  interval=2s  health_window=10 cycles  lifecycle_interval=3600s
[2026-04-25T21:26:41Z INFO ] - linux disk space manager - "db45prod" - watching '/var' — 2.3 GiB/9.1 GiB used, 4 threshold(s) configured
[2026-04-25T21:26:41Z INFO ] - linux disk space manager - "db45prod" - watching '/' — 19.8 GiB/22.7 GiB used, 2 threshold(s) configured
[2026-04-25T21:26:41Z INFO ] - linux disk space manager - "db45prod" - watching '/tmp' — 111.3 MiB/1.8 GiB used, 3 threshold(s) configured
[2026-04-25T21:27:33Z WARN ] - linux disk space manager - "db45prod" - [/tmp] disk at 97.8% — 75% threshold breached (cycle 1/10)
[2026-04-25T21:27:33Z WARN ] - linux disk space manager - "db45prod" - [/tmp] disk at 97.8% — 90% threshold breached (cycle 1/10)
[2026-04-25T21:27:37Z WARN ] - linux disk space manager - "db45prod" - [/tmp] disk at 100.0% — 99% threshold breached (cycle 1/10)
[2026-04-25T21:27:51Z WARN ] - linux disk space manager - "db45prod" - [/tmp] disk sustained at 100.0% >= 75% for 10 cycle(s) — running 1 reaction command(s)
[2026-04-25T21:27:51Z INFO ] - linux disk space manager - "db45prod" - running reaction: find /tmp -mindepth 1 -mtime +7 -delete
[2026-04-25T21:27:51Z WARN ] - linux disk space manager - "db45prod" - [/tmp] disk sustained at 100.0% >= 90% for 10 cycle(s) — running 1 reaction command(s)
[2026-04-25T21:27:51Z INFO ] - linux disk space manager - "db45prod" - running reaction: find /tmp -mindepth 1 -mtime +1 -delete
[2026-04-25T21:27:55Z WARN ] - linux disk space manager - "db45prod" - [/tmp] disk sustained at 100.0% >= 99% for 10 cycle(s) — running 1 reaction command(s)
[2026-04-25T21:27:55Z INFO ] - linux disk space manager - "db45prod" - running reaction: rm -rf /tmp/*
[2026-04-25T21:27:57Z INFO ] - linux disk space manager - "db45prod" - [/tmp] recovered below 75% threshold — disk now at 6.1% (111.3 MiB/1.8 GiB)
[2026-04-25T21:27:57Z INFO ] - linux disk space manager - "db45prod" - [/tmp] recovered below 90% threshold — disk now at 6.1% (111.3 MiB/1.8 GiB)
[2026-04-25T21:27:57Z INFO ] - linux disk space manager - "db45prod" - [/tmp] recovered below 99% threshold — disk now at 6.1% (111.3 MiB/1.8 GiB)

```

There is also quiet mode (-q) and debug mode (-d). Quiet mode is ueful for systems that are sensitive for IO or systems where the daemon is very active
and we don't want the linux-disk-space-manager logs to contribute to disk space issues themselves!

## example of running in the foreground in debug mode

While the program is designed to be a daemon and run in the background, the program can be run manually in the foreground of a terminal session instead.

This is especially useful for test and QA systems, testing the policy YAML and making sure the handling and data retention are as desired.

Here is an example of running the linux-disk-space-manager manually in debug mode and having /tmp fill to 100% with policy that effectively cleans up from that condition:

```
$ linux-disk-space-manager policy.yaml -d 2>&1 | tee disk_manager_$(date +%Y%m%d%H%M%S).log 
[2026-04-25T21:13:07Z INFO ] - "db45prod" - linux-disk-space-manager started  policy=policy.yaml  interval=2s  health_window=10 cycles  lifecycle_interval=3600s
[2026-04-25T21:13:07Z INFO ] - "db45prod" - watching '/var' — 2.3 GiB/9.1 GiB used, 4 threshold(s) configured
[2026-04-25T21:13:07Z INFO ] - "db45prod" - watching '/' — 19.8 GiB/22.7 GiB used, 2 threshold(s) configured
[2026-04-25T21:13:07Z INFO ] - "db45prod" - watching '/tmp' — 111.3 MiB/1.8 GiB used, 3 threshold(s) configured
[2026-04-25T21:13:07Z DEBUG] - "db45prod" - lifecycle: starting scheduled run
[2026-04-25T21:13:07Z DEBUG] - "db45prod" - lifecycle: running 7 rule(s)
[2026-04-25T21:13:07Z DEBUG] - "db45prod" - lifecycle: ok [69 days, 0 MiB] /var/log/postgresql/postgresql-15-main.log.10.gz
[2026-04-25T21:13:07Z DEBUG] - "db45prod" - lifecycle: ok [13 days, 0 MiB] /var/log/postgresql/postgresql-15-main.log.2.gz
[2026-04-25T21:13:07Z DEBUG] - "db45prod" - lifecycle: ok [20 days, 0 MiB] /var/log/postgresql/postgresql-15-main.log.3.gz
[2026-04-25T21:13:07Z DEBUG] - "db45prod" - lifecycle: ok [27 days, 0 MiB] /var/log/postgresql/postgresql-15-main.log.4.gz
[2026-04-25T21:13:07Z DEBUG] - "db45prod" - lifecycle: ok [34 days, 0 MiB] /var/log/postgresql/postgresql-15-main.log.5.gz
[2026-04-25T21:13:07Z DEBUG] - "db45prod" - lifecycle: ok [41 days, 0 MiB] /var/log/postgresql/postgresql-15-main.log.6.gz
[2026-04-25T21:13:07Z DEBUG] - "db45prod" - lifecycle: ok [48 days, 0 MiB] /var/log/postgresql/postgresql-15-main.log.7.gz
[2026-04-25T21:13:07Z DEBUG] - "db45prod" - lifecycle: ok [55 days, 0 MiB] /var/log/postgresql/postgresql-15-main.log.8.gz
[2026-04-25T21:13:07Z DEBUG] - "db45prod" - lifecycle: ok [62 days, 0 MiB] /var/log/postgresql/postgresql-15-main.log.9.gz
[2026-04-25T21:13:07Z DEBUG] - "db45prod" - [/var] 24.9% used (2.3 GiB/9.1 GiB)
[2026-04-25T21:13:07Z DEBUG] - "db45prod" - [/] 87.1% used (19.8 GiB/22.7 GiB)
[2026-04-25T21:13:07Z DEBUG] - "db45prod" - [/tmp] 6.1% used (111.3 MiB/1.8 GiB)
[2026-04-25T21:13:07Z DEBUG] - "db45prod" - cycle done in 0ms — sleeping 1999ms
...
[2026-04-25T21:15:11Z DEBUG] - "db45prod" - [/var] 24.9% used (2.3 GiB/9.1 GiB)
[2026-04-25T21:15:11Z DEBUG] - "db45prod" - [/] 87.1% used (19.8 GiB/22.7 GiB)
[2026-04-25T21:15:11Z DEBUG] - "db45prod" - [/tmp] 92.2% used (1.7 GiB/1.8 GiB)
[2026-04-25T21:15:11Z WARN ] - "db45prod" - [/tmp] disk at 92.2% — 75% threshold breached (cycle 1/10)
[2026-04-25T21:15:11Z WARN ] - "db45prod" - [/tmp] disk at 92.2% — 90% threshold breached (cycle 1/10)
[2026-04-25T21:15:11Z DEBUG] - "db45prod" - cycle done in 0ms — sleeping 1999ms
[2026-04-25T21:15:13Z DEBUG] - "db45prod" - [/var] 24.9% used (2.3 GiB/9.1 GiB)
[2026-04-25T21:15:13Z DEBUG] - "db45prod" - [/] 87.1% used (19.8 GiB/22.7 GiB)
[2026-04-25T21:15:13Z DEBUG] - "db45prod" - [/tmp] 100.0% used (1.8 GiB/1.8 GiB)
[2026-04-25T21:15:13Z WARN ] - "db45prod" - [/tmp] disk at 100.0% — 99% threshold breached (cycle 1/10)
[2026-04-25T21:15:13Z DEBUG] - "db45prod" - cycle done in 0ms — sleeping 1999ms
[2026-04-25T21:15:15Z DEBUG] - "db45prod" - [/var] 24.9% used (2.3 GiB/9.1 GiB)
[2026-04-25T21:15:15Z DEBUG] - "db45prod" - [/] 87.1% used (19.8 GiB/22.7 GiB)
[2026-04-25T21:15:15Z DEBUG] - "db45prod" - [/tmp] 100.0% used (1.8 GiB/1.8 GiB)
[2026-04-25T21:15:15Z DEBUG] - "db45prod" - cycle done in 0ms — sleeping 1999ms
[2026-04-25T21:15:17Z DEBUG] - "db45prod" - [/var] 24.9% used (2.3 GiB/9.1 GiB)
[2026-04-25T21:15:17Z DEBUG] - "db45prod" - [/] 87.1% used (19.8 GiB/22.7 GiB)
[2026-04-25T21:15:17Z DEBUG] - "db45prod" - [/tmp] 100.0% used (1.8 GiB/1.8 GiB)
[2026-04-25T21:15:17Z DEBUG] - "db45prod" - cycle done in 0ms — sleeping 1999ms
[2026-04-25T21:15:19Z DEBUG] - "db45prod" - [/var] 24.9% used (2.3 GiB/9.1 GiB)
[2026-04-25T21:15:19Z DEBUG] - "db45prod" - [/] 87.1% used (19.8 GiB/22.7 GiB)
[2026-04-25T21:15:19Z DEBUG] - "db45prod" - [/tmp] 100.0% used (1.8 GiB/1.8 GiB)
[2026-04-25T21:15:19Z DEBUG] - "db45prod" - cycle done in 0ms — sleeping 1999ms
[2026-04-25T21:15:21Z DEBUG] - "db45prod" - [/var] 24.9% used (2.3 GiB/9.1 GiB)
[2026-04-25T21:15:21Z DEBUG] - "db45prod" - [/] 87.1% used (19.8 GiB/22.7 GiB)
[2026-04-25T21:15:21Z DEBUG] - "db45prod" - [/tmp] 100.0% used (1.8 GiB/1.8 GiB)
[2026-04-25T21:15:21Z DEBUG] - "db45prod" - cycle done in 0ms — sleeping 1999ms
[2026-04-25T21:15:23Z DEBUG] - "db45prod" - [/var] 24.9% used (2.3 GiB/9.1 GiB)
[2026-04-25T21:15:23Z DEBUG] - "db45prod" - [/] 87.1% used (19.8 GiB/22.7 GiB)
[2026-04-25T21:15:23Z DEBUG] - "db45prod" - [/tmp] 100.0% used (1.8 GiB/1.8 GiB)
[2026-04-25T21:15:23Z DEBUG] - "db45prod" - cycle done in 0ms — sleeping 1999ms
[2026-04-25T21:15:25Z DEBUG] - "db45prod" - [/var] 24.9% used (2.3 GiB/9.1 GiB)
[2026-04-25T21:15:25Z DEBUG] - "db45prod" - [/] 87.1% used (19.8 GiB/22.7 GiB)
[2026-04-25T21:15:25Z DEBUG] - "db45prod" - [/tmp] 100.0% used (1.8 GiB/1.8 GiB)
[2026-04-25T21:15:25Z DEBUG] - "db45prod" - cycle done in 0ms — sleeping 1999ms
[2026-04-25T21:15:27Z DEBUG] - "db45prod" - [/var] 24.9% used (2.3 GiB/9.1 GiB)
[2026-04-25T21:15:27Z DEBUG] - "db45prod" - [/] 87.1% used (19.8 GiB/22.7 GiB)
[2026-04-25T21:15:27Z DEBUG] - "db45prod" - [/tmp] 100.0% used (1.8 GiB/1.8 GiB)
[2026-04-25T21:15:27Z DEBUG] - "db45prod" - cycle done in 0ms — sleeping 1999ms
[2026-04-25T21:15:29Z DEBUG] - "db45prod" - [/var] 24.9% used (2.3 GiB/9.1 GiB)
[2026-04-25T21:15:29Z DEBUG] - "db45prod" - [/] 87.1% used (19.8 GiB/22.7 GiB)
[2026-04-25T21:15:29Z DEBUG] - "db45prod" - [/tmp] 100.0% used (1.8 GiB/1.8 GiB)
[2026-04-25T21:15:29Z WARN ] - "db45prod" - [/tmp] disk sustained at 100.0% >= 75% for 10 cycle(s) — running 1 reaction command(s)
[2026-04-25T21:15:29Z INFO ] - "db45prod" - running reaction: find /tmp -mindepth 1 -mtime +7 -delete
[2026-04-25T21:15:29Z DEBUG] - "db45prod" -   ok (exit 0): find /tmp -mindepth 1 -mtime +7 -delete
[2026-04-25T21:15:29Z WARN ] - "db45prod" - [/tmp] disk sustained at 100.0% >= 90% for 10 cycle(s) — running 1 reaction command(s)
[2026-04-25T21:15:29Z INFO ] - "db45prod" - running reaction: find /tmp -mindepth 1 -mtime +1 -delete
[2026-04-25T21:15:29Z DEBUG] - "db45prod" -   ok (exit 0): find /tmp -mindepth 1 -mtime +1 -delete
[2026-04-25T21:15:29Z DEBUG] - "db45prod" - cycle done in 3ms — sleeping 1996ms
[2026-04-25T21:15:31Z DEBUG] - "db45prod" - [/var] 24.9% used (2.3 GiB/9.1 GiB)
[2026-04-25T21:15:31Z DEBUG] - "db45prod" - [/] 87.1% used (19.8 GiB/22.7 GiB)
[2026-04-25T21:15:31Z DEBUG] - "db45prod" - [/tmp] 100.0% used (1.8 GiB/1.8 GiB)
[2026-04-25T21:15:31Z WARN ] - "db45prod" - [/tmp] disk sustained at 100.0% >= 99% for 10 cycle(s) — running 1 reaction command(s)
[2026-04-25T21:15:31Z INFO ] - "db45prod" - running reaction: rm -rf /tmp/*
[2026-04-25T21:15:31Z DEBUG] - "db45prod" -   ok (exit 0): rm -rf /tmp/*
[2026-04-25T21:15:31Z DEBUG] - "db45prod" - cycle done in 69ms — sleeping 1930ms
[2026-04-25T21:15:33Z DEBUG] - "db45prod" - [/var] 24.9% used (2.3 GiB/9.1 GiB)
[2026-04-25T21:15:33Z DEBUG] - "db45prod" - [/] 87.1% used (19.8 GiB/22.7 GiB)
[2026-04-25T21:15:33Z DEBUG] - "db45prod" - [/tmp] 6.1% used (111.3 MiB/1.8 GiB)
[2026-04-25T21:15:33Z INFO ] - "db45prod" - [/tmp] recovered below 75% threshold — disk now at 6.1% (111.3 MiB/1.8 GiB)
[2026-04-25T21:15:33Z INFO ] - "db45prod" - [/tmp] recovered below 90% threshold — disk now at 6.1% (111.3 MiB/1.8 GiB)
[2026-04-25T21:15:33Z INFO ] - "db45prod" - [/tmp] recovered below 99% threshold — disk now at 6.1% (111.3 MiB/1.8 GiB)
[2026-04-25T21:15:33Z DEBUG] - "db45prod" - cycle done in 0ms — sleeping 1999ms
[2026-04-25T21:15:35Z DEBUG] - "db45prod" - [/var] 24.9% used (2.3 GiB/9.1 GiB)
[2026-04-25T21:15:35Z DEBUG] - "db45prod" - [/] 87.1% used (19.8 GiB/22.7 GiB)
[2026-04-25T21:15:35Z DEBUG] - "db45prod" - [/tmp] 6.1% used (111.3 MiB/1.8 GiB)
[2026-04-25T21:15:35Z DEBUG] - "db45prod" - cycle done in 0ms — sleeping 1999ms
[2026-04-25T21:15:37Z DEBUG] - "db45prod" - [/var] 24.9% used (2.3 GiB/9.1 GiB)
[2026-04-25T21:15:37Z DEBUG] - "db45prod" - [/] 87.1% used (19.8 GiB/22.7 GiB)
[2026-04-25T21:15:37Z DEBUG] - "db45prod" - [/tmp] 6.1% used (111.3 MiB/1.8 GiB)
[2026-04-25T21:15:37Z DEBUG] - "db45prod" - cycle done in 0ms — sleeping 1999ms
...
```

