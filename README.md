# 📟 Alias Manager

_The_ solution to a problem you don't have -- a TUI for managing your shell aliases. View, search, and audit all your **Zsh**, **Bash**, and **Fish** aliases in one place.

Built with Rust and Ratatui. **Supports macOS and Linux.**

<img width="3396" height="1652" alt="CleanShot 2026-02-19 at 20 44 43@2x" src="https://github.com/user-attachments/assets/a5941e2a-21af-4ec7-a997-f338c6f129b7" />


## ✨ Features

- **🚀 Multi-Shell Discovery**: Automatically parses `.zshrc`, `.bashrc`, `.profile`, and `config.fish`.
- **🌿 Git Integration**: Full support for `~/.gitconfig` aliases.
- **🔍 Global Search**: Instant filtering across alias names, expanded commands, tags, or source files.
- **🏥 Health Checks**: Automatically detects "broken" aliases where the underlying command is missing from your `$PATH`.
- **🔗 Shadow Chains**: Identifies when aliases are overridden across multiple configuration files.
- **📊 Usage Analytics**: Correlates shell history to show total usage counts and "last used" timestamps.
- **🏷️ Categorization**: Supports `# @tag` comments for custom organization.
- **📋 Dual Yanking**: Copy the alias name (`y`) or the fully resolved expansion (`Y`).
- **📝 Instant Edit**: Jump directly to the source file and line of any alias using your `$EDITOR`.

## 🎮 Controls

| Key | Action |
|-----|--------|
| `Tab` | Switch focus between Filter and Aliases |
| `j` / `k` | Navigate the alias list |
| `s` | Toggle Sort (Name → Usage → Health) |
| `d` | Toggle Details pane |
| `h` | Toggle source filenames |
| `y` | Yank alias name |
| `Y` | Yank expanded command |
| `e` | Edit source file at line |
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
alias-manager --dump
```

## 🤝 Contributing

Contributions are welcome! 

If you're looking to add a new source (e.g., a new shell or config format):
1. Create a parser in `src/parser/your_source.rs`.
2. Register it in `src/parser/mod.rs`.
3. Call it in the `get_all_aliases()` discovery loop.

## 📄 License

MIT
