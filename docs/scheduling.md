# Scheduling Reminders

knotter does not run a background daemon. Use your system scheduler to run reminders.

For reminder output details, see [Knotter CLI Output](cli-output.md).

## Cron (daily at 09:00)

```
0 9 * * * /path/to/knotter remind
```

## systemd user timer

Create `~/.config/systemd/user/knotter-remind.service`:

```
[Unit]
Description=knotter reminders

[Service]
Type=oneshot
ExecStart=/path/to/knotter remind
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

## Use cases

### 1) Plain stdout list (no notifications)

No config required. `knotter remind` prints a human-readable list to stdout.

```
0 9 * * * /path/to/knotter remind
```

### 2) Desktop notifications

Build with the `desktop-notify` feature and enable the backend in config:

```
[notifications]
enabled = true
backend = "desktop"
```

Then schedule:

```
0 9 * * * /path/to/knotter remind
```

### 3) Email notifications (SMTP)

Build with the `email-notify` feature and configure SMTP in your config:

```
[notifications]
enabled = true
backend = "email"

[notifications.email]
from = "Knotter <knotter@example.com>"
to = ["you@example.com"]
subject_prefix = "knotter reminders"
smtp_host = "smtp.example.com"
smtp_port = 587
username = "user@example.com"
password_env = "KNOTTER_SMTP_PASSWORD"
tls = "start-tls" # start-tls | tls | none
timeout_seconds = 20
```

Provide the password via env var in your scheduler:

```
KNOTTER_SMTP_PASSWORD=your-app-password
0 9 * * * /path/to/knotter remind
```

For systemd, add an environment line to the service:

```
[Service]
Type=oneshot
Environment=KNOTTER_SMTP_PASSWORD=your-app-password
ExecStart=/path/to/knotter remind
```

Or use an environment file:

```
[Service]
Type=oneshot
EnvironmentFile=%h/.config/knotter/knotter.env
ExecStart=/path/to/knotter remind
```

### 4) JSON automation

Use JSON output for automation. Notifications only run when `--notify` is set:

```
/path/to/knotter remind --json
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
- If notifications are enabled in config, `--notify` is optional for non-JSON
  runs. Use `--notify` to force notifications even when config is disabled.
- If notifications are enabled in config, pass `--no-notify` to suppress them
  for a single run.
- If `notifications.backend = "stdout"`, `--notify` prints the full reminder list
  (same as human output). This backend cannot be used with `--json`.
- For email delivery, build with the `email-notify` feature and configure
  `[notifications.email]` in your config; secrets should be provided via
  `password_env` (see README).
