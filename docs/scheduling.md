# Scheduling reminders

knotter does not run a background daemon. Use your system scheduler to run reminders.

## Cron (daily at 09:00)

```
0 9 * * * /path/to/knotter remind --notify
```

## systemd user timer

Create `~/.config/systemd/user/knotter-remind.service`:

```
[Unit]
Description=knotter reminders

[Service]
Type=oneshot
ExecStart=/path/to/knotter remind --notify
```

Create `~/.config/systemd/user/knotter-remind.timer`:

```
[Unit]
Description=Run knotter reminders daily

[Timer]
OnCalendar=*-*-* 09:00:00
Persistent=true

[Install]
WantedBy=timers.target
```

Enable it:

```
systemctl --user daemon-reload
systemctl --user enable --now knotter-remind.timer
```

## Notes

- `knotter remind` prints human output to stdout unless `--json` or `--notify` is used.
- `knotter remind --json` always emits JSON to stdout, even with `--notify`.
- `knotter remind --notify` will use desktop notifications if built with the
  `desktop-notify` feature; otherwise it falls back to a stdout summary.
- `knotter remind --json` emits JSON for automation; notifications still run, but
  notification failure returns a non-zero exit code to avoid silent misses.
