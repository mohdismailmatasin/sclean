# sclean

A lightweight system cleanup utility for Linux that safely removes temporary files, caches, and other reclaimable disk space.

## Features

- **44 built-in cleanup targets** covering system, development, and application data
- **Safe by design** — skips protected directories and requires confirmation by default
- **Dry-run mode** — preview what will be deleted without making changes
- **Interactive mode** — choose which items to clean individually
- **Highly configurable** — custom targets, age limits, and protected paths
- **Fast and efficient** — written in Rust with minimal dependencies

## Quick Start

```bash
# Install
curl -fsSL https://raw.githubusercontent.com/mohdismailmatasin/sclean/main/install.sh | sudo sh

# Clean everything
sclean

# Preview what would be deleted (dry-run)
sclean --preview

# Interactive mode - choose what to clean
sclean --interactive
```

## Installation

### From Script (Recommended)

```bash
curl -fsSL https://raw.githubusercontent.com/mohdismailmatasin/sclean/main/install.sh | sudo sh
```

### From Source

```bash
cargo install sclean
```

### From GitHub Releases

Download the latest binary from the [releases page](https://github.com/mohdismailmatasin/sclean/releases) and place it in your PATH.

## Usage

```bash
# Clean all targets
sclean

# Preview without deleting
sclean --preview

# Verbose output
sclean --verbose

# Quiet mode (minimal output)
sclean --quiet

# Interactive mode - prompt before each cleanup
sclean --interactive

# Clean specific targets only
sclean --targets cache,dev,browser

# List all available targets
sclean --list-targets

# Generate default config file
sclean --generate-config
```

## Command-Line Options

| Option              | Description                                 |
| ------------------- | ------------------------------------------- |
| `-p, --preview`     | Show what would be cleaned without deleting |
| `-v, --verbose`     | Enable detailed output                      |
| `-q, --quiet`       | Suppress most output                        |
| `-i, --interactive` | Prompt before each cleanup                  |
| `-t, --targets`     | Comma-separated list of targets to clean    |
| `--list-targets`    | Display all available cleanup targets       |
| `--generate-config` | Create default configuration file           |

## Configuration

Create or generate a config file at `~/.config/sclean/config.toml`:

```bash
sclean --generate-config
```

### Configuration Options

| Option              | Type    | Default                         | Description                         |
| ------------------- | ------- | ------------------------------- | ----------------------------------- |
| `max_log_age_days`  | integer | `7`                             | Remove logs older than N days       |
| `max_temp_age_days` | integer | `1`                             | Remove temp files older than N days |
| `protected_dirs`    | array   | `["Desktop", "Downloads", ...]` | Directories to never clean          |
| `targets`           | array   | `[]`                            | Custom cleanup targets              |

### Configuration Example

```toml
max_log_age_days = 7
max_temp_age_days = 1

protected_dirs = ["Desktop", "Downloads", "Documents", "Pictures", "Videos", "Music", "Templates"]

[[targets]]
name = "Custom cache"
path = "/home/user/my-cache"
enabled = true
```

## Cleanup Targets

sclean includes **44 built-in targets** organized by category:

| Category         | Targets                                                    |
| ---------------- | ---------------------------------------------------------- |
| **System**       | APT, pacman, flatpak, snap, systemd journal, core dumps    |
| **Temp & Cache** | `/tmp`, `/var/tmp`, user cache, thumbnails                 |
| **Browsers**     | Firefox, Chrome, Chromium                                  |
| **Dev Tools**    | npm, yarn, bun, pnpm, pip, poetry, uv, cargo, composer, go |
| **Applications** | Discord, Slack, VS Code, JetBrains, Spotify, Steam         |
| **GPU**          | Mesa and NVIDIA shader caches                              |
| **Desktop**      | Trash, recent files, font cache, file indexer              |
| **Arch Linux**   | Orphan packages, old pacman versions, lock file            |
| **Docker**       | System prune (disabled by default)                         |

## Uninstallation

```bash
curl -fsSL https://raw.githubusercontent.com/mohdismailmatasin/sclean/main/uninstall.sh | sudo sh
```

Or remove manually:

```bash
sudo rm /usr/local/bin/sclean
sudo rm -rf /usr/local/share/doc/sclean
```

## License

MIT License - see [LICENSE](LICENSE) for details.

## Contributing

Contributions are welcome! Please read the [contributing guidelines](CONTRIBUTING.md) before submitting PRs.