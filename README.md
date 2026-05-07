# boot-dash

Systemd boot and service dashboard TUI -- three-tab view of services, boot blame, and journal logs.

## Features

- **Services tab**: lists all systemd units with state, sub-state, and description; color-coded by status
- **Boot tab**: shows total/kernel/userspace/firmware boot times and top-30 `systemd-analyze blame` entries
- **Journal tab**: recent 200 journal entries with priority-based coloring (red for errors, yellow for warnings)
- Filter services by name or description with `/`
- Header displays hostname, kernel version, uptime, and systemd version
- Auto-refreshes every 5 seconds; manual refresh with `r`
- Tab or `1`/`2`/`3` to switch views
- Vim-style scrolling with `g`/`G` for top/bottom
- Phosphor-green on black aesthetic

## Install

```
cargo build --release
cp target/release/boot-dash ~/.local/bin/
```

## Usage

```bash
boot-dash    # launch the dashboard
```

## Keybindings

| Key | Action |
|-----|--------|
| `1` / `2` / `3` | Switch to Services / Boot / Journal tab |
| `Tab` | Cycle tabs |
| `j` / `k` | Scroll down / up |
| `g` / `G` | Jump to top / bottom |
| `/` | Filter (Services tab) |
| `r` | Manual refresh |
| `q` | Quit |

Built with Rust + ratatui.
