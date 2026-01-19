# Scheduling Reminders

knotter does not run a background daemon. Use your system scheduler to run reminders.

For reminder output details, see [Knotter CLI Output](cli-output.md).

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

- `knotter remind` prints human output to stdout unless `--json` is used. If
  notifications are enabled (via `--notify` or config), it will notify instead
  of printing the list.
- `knotter remind --json` always emits JSON to stdout. Notifications only run when
  `--notify` is provided explicitly.
- `knotter remind --notify` will use desktop notifications if built with the
  `desktop-notify` feature; otherwise it falls back to a stdout summary.
- `knotter remind --json` emits JSON for automation; with `--notify`, notification
  failure returns a non-zero exit code to avoid silent misses.
- Configure defaults in `~/.config/knotter/config.toml` (see README) for
  `due_soon_days` and notification settings.
- If notifications are enabled in config, pass `--no-notify` to suppress them
  for a single run.
- If `notifications.backend = "stdout"`, `--notify` prints the full reminder list
  (same as human output). This backend cannot be used with `--json`.
