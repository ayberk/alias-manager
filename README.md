# 📟 Alias Manager

_The_ solution to a problem you don't have -- a TUI for managing your shell aliases. View, search, and audit all your **Zsh**, **Bash**, and **Fish** aliases in one place.

Built with Rust and Ratatui. **Supports macOS and Linux.**

<img width="3396" height="1652" alt="CleanShot 2026-02-19 at 20 44 43@2x" src="https://github.com/user-attachments/assets/a5941e2a-21af-4ec7-a997-f338c6f129b7" />


## ✨ Features

- **🚀 Multi-Shell Discovery**: Automatically parses `.zshrc`, `.bashrc`, `.profile`, and `config.fish`. Follows `source` and `.` directives recursively so aliases defined in included files are discovered too.
- **❄️ Nix Integration**: Reads `environment.shellAliases` (and `programs.*.shellAliases`) from all `.nix` files under `~/.config/nix/`, plus common nix-darwin and home-manager locations. Also picks up `alias name=value` lines inside nix string literals (e.g. `interactiveShellInit`).
- **🌿 Git Integration**: Full support for `~/.gitconfig` aliases.
- **🔍 Global Search**: Instant filtering across alias names, expanded commands, tags, descriptions, or source files. Prefix a query with `@` to filter by tag only (e.g. `@work`).
- **🏥 Health Checks**: Automatically detects "broken" aliases where the underlying command is missing from your `$PATH`.
- **🔗 Shadow Chains**: Identifies when aliases are overridden across multiple configuration files and shows the full shadow chain in the Details pane.
- **♊ Duplicate Detection**: Flags aliases that resolve to the exact same command across different names.
- **📊 Usage Analytics**: Correlates Zsh, Bash, and Fish history to show total usage counts and "last used" timestamps.
- **🏷️ Tagging & Descriptions**: Add `# @tag` words before an alias definition for custom organization. Any non-tag comment words become a description shown in the Details pane.
- **📋 Dual Yanking**: Copy the alias name (`y`) or the fully resolved expansion (`Y`).
- **📝 Instant Edit**: Jump directly to the source file and line of any alias using your `$EDITOR`.
- **🚫 Source Exclusion**: Permanently hide all aliases from a noisy source file with a single keypress.

## 🎮 Controls

| Key | Action |
|-----|--------|
| `Tab` | Switch focus between Filter and Aliases |
| `j` / `k` | Navigate the alias list |
| `s` | Toggle Sort (Name → Usage → Health) |
| `d` | Toggle Details pane |
| `h` | Toggle source filenames |
| `y` | Yank alias name to clipboard |
| `Y` | Yank fully expanded command to clipboard |
| `e` | Edit source file at alias line in `$EDITOR` |
| `x` | Exclude all aliases from this source file (saved to config) |
| `/` | Focus search filter |
| `c` | Clear search filter |
| `?` | Toggle help legend |
| `q` | Quit |

## 🛠️ Requirements

None, really. You need a [nerd font](https://www.nerdfonts.com) for source file icons, though.

## 📦 Installation

### Cargo (Rust Users)
```bash
cargo install als-manager
```

### Homebrew
```bash
brew tap ayberk/tap
brew install alias-manager
```

### Nix (Flakes)
```bash
nix run github:ayberk/alias-manager
```

### Manual (From Source)
```bash
git clone https://github.com/ayberk/alias-manager.git
cd alias-manager
cargo build --release
```

## 🚀 Usage

**Interactive Mode:**
```bash
alias-manager
```

**CLI Dump (Non-interactive):**
```bash
alias-manager --dump           # table format (default)
alias-manager --dump=json      # JSON array
alias-manager --dump=csv       # CSV with header row
```

All dump formats output `name`, `source`, and the fully expanded command (falls back to the raw command if no expansion is possible). Results are sorted alphabetically by alias name.

## ⚙️ Configuration

Alias Manager reads `~/.alias-manager.conf` on startup. All keys are optional and the file is written back automatically when you change settings from within the TUI.

| Key | Default | Description |
|-----|---------|-------------|
| `show_source` | `true` | Show source filenames in the list |
| `show_help` | `true` | Show the help legend at startup |
| `last_filter` | _(empty)_ | Restore the last search filter on launch |
| `nix_paths` | _(empty)_ | Colon-separated extra nix files or directories to scan |
| `ignore_sources` | _(empty)_ | Colon-separated source files whose aliases are hidden |

**Example:**
```ini
show_source=true
show_help=true
last_filter=git
nix_paths=~/dotfiles/nix:~/work/nix-config/modules/aliases.nix
ignore_sources=~/.iterm2_shell_integration.zsh
```

`nix_paths` entries can be individual `.nix` files or directories (scanned recursively up to 3 levels). These are in addition to the built-in defaults — `~/.config/nix/`, `~/.nixpkgs/darwin-configuration.nix`, and `~/.config/nixpkgs/home.nix` — which are always scanned.

`ignore_sources` is also written automatically when you press `x` on an alias in the TUI.

## 🏷️ Tagging Aliases

Place a comment on the line immediately before an alias to attach metadata:

```zsh
# @work @git Quick status shortcut
alias gs='git status'
```

- Words starting with `@` become searchable **tags**.
- Remaining words become a **description** shown in the Details pane.
- In the filter box, prefix your query with `@` to match tags only — e.g. `@work` shows only aliases tagged `work`.

This works in Zsh/Bash shell files, `.gitconfig`, and `.nix` files.

## 🤝 Contributing

Contributions are welcome! 

If you're looking to add a new source (e.g., a new shell or config format):
1. Create a parser in `src/parser/your_source.rs`.
2. Register it in `src/parser/mod.rs`.
3. Call it in the `get_all_aliases(extra_nix_paths)` discovery loop.

## 📄 License

MIT
