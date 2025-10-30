# AGPM - AGentic Package Manager

> ⚠️ **Beta Software**: This project is in active development. Use with caution in production environments.

A Git-based package manager for AI coding assistants (Claude Code, OpenCode, and more) that enables reproducible
installations using lockfile-based dependency management, similar to Cargo.

## Features

- 📦 **Lockfile-based dependency management** - Reproducible installations like Cargo
- 🌐 **Git-based distribution** - Install from any Git repository
- 🚀 **No central registry** - Fully decentralized approach
- 🤖 **Multi-tool support** - Claude Code, OpenCode (alpha), and custom tools
- 🔧 **Seven resource types** - Agents, Snippets, Commands, Scripts, Hooks, MCP Servers, Skills
- 🎯 **Pattern-based dependencies** - Bulk installation with glob patterns
- 🖥️ **Cross-platform** - Windows, macOS, and Linux support
- 🔄 **Transitive dependencies** - Automatic dependency resolution
- 📝 **Markdown templating** - Dynamic content generation with dependency embedding and project file filter (opt-in)

## Quick Start

### Install

```bash
# macOS/Linux via Homebrew
brew install aig787/homebrew-agpm/agpm-cli

# All platforms via Cargo
cargo install agpm-cli

# Pre-built binaries
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/aig787/agpm/releases/latest/download/agpm-installer.sh | sh
```

See the [Installation Guide](docs/installation.md) for more options and platform-specific instructions.

### Basic Usage

```bash
# Initialize a new project
agpm init

# Install dependencies
agpm install

# Check for updates
agpm outdated

# Update dependencies
agpm update

# List installed resources
agpm list
```

### Example Manifest

```toml
# agpm.toml
[sources]
community = "https://github.com/aig787/agpm-community.git"

[agents]
# Claude Code agent (default)
rust-expert = { source = "community", path = "agents/rust-expert.md", version = "v1.0.0" }

# OpenCode agent (alpha)
assistant-oc = { source = "community", path = "agents/assistant.md", version = "v1.0.0", tool = "opencode" }

[snippets]
# Shared snippets (default: .agpm/snippets/)
react-hooks = { source = "community", path = "snippets/react-hooks.md", version = "~1.2.0" }

[commands]
deploy = { source = "community", path = "commands/deploy.md", version = "v2.0.0" }

[skills]
# Claude Skills - directory-based expertise packages
rust-helper = { source = "community", path = "skills/rust-helper", version = "v1.0.0" }
ai-reviewer = { source = "community", path = "skills/ai-reviewer", version = "^2.1.0" }
```

See [docs/examples/](docs/examples/) for more complete examples.

## Core Commands

| Command         | Description                                       |
|-----------------|---------------------------------------------------|
| `agpm init`     | Initialize a new project                          |
| `agpm install`  | Install dependencies from agpm.toml               |
| `agpm update`   | Update dependencies within version constraints    |
| `agpm outdated` | Check for available updates                       |
| `agpm upgrade`  | Self-update AGPM to the latest version            |
| `agpm list`     | List installed resources                          |
| `agpm validate` | Validate manifest and dependencies                |
| `agpm add`      | Add sources or dependencies                       |
| `agpm remove`   | Remove sources or dependencies                    |
| `agpm config`   | Manage global configuration                       |
| `agpm cache`    | Manage the Git cache                              |

Run `agpm --help` for complete command reference or see [Command Reference](docs/command-reference.md).

## Progress Display

AGPM provides real-time visibility into installation progress with a clean, professional interface:

### Installation Phases

```
⠁ Syncing sources
✓ Sources synced (0.8s)

⠂ Resolving dependencies
✓ Resolved 500 dependencies (1.2s)

⠄ Installing resources (127/500 complete)
  → agents/helper-122
  → agents/helper-123
  → agents/helper-124
  → snippets/example-45
  → commands/lint-67
  → agents/helper-125
  → agents/helper-126

✓ Installed 500 resources (12.3s)
  ✓ 300 agents
  ✓ 150 snippets
  ✓ 50 commands

✓ Finalizing installation (0.2s)

  500 resources installed
  2 MCP servers configured
```

### Features

- **Active Window**: Shows which resources are currently being processed (5-10 lines)
- **Real-time Updates**: Resources appear and complete in real-time
- **Timing Information**: Each phase shows duration for performance insights
- **Bounded Output**: Terminal stays clean regardless of dependency count
- **Resource Summary**: Final breakdown by resource type (agents, snippets, etc.)
- **Professional Display**: Clean output without emoji prefixes

## Resource Types

AGPM manages seven types of resources:

- **Agents** - AI assistant configurations (`.claude/agents/`, `.opencode/agent/`)
- **Snippets** - Reusable code templates (`.agpm/snippets/`)
- **Commands** - Slash commands (`.claude/commands/`, `.opencode/command/`)
- **Scripts** - Executable automation files (`.claude/scripts/`)
- **Hooks** - Event-based automation (→ `.claude/settings.local.json`)
- **MCP Servers** - Model Context Protocol servers (→ `.mcp.json`, `opencode.json`)
- **Skills** - Directory-based expertise packages with SKILL.md (`.claude/skills/`)

See the [Resources Guide](docs/resources.md) for detailed information.

## Templating Features

AGPM provides powerful template features for dynamic content generation in Markdown resources:

### Dependency Content Embedding

Embed versioned content from AGPM dependencies:

```markdown
---
agpm.templating: true
dependencies:
  snippets:
    - path: snippets/rust-patterns.md
      name: rust_patterns
---
# Rust Code Reviewer

## Shared Patterns
{{ agpm.deps.snippets.rust_patterns.content }}
```

### Project File Filter

Read and embed project-specific files (team docs, company standards):

```markdown
---
agpm.templating: true
---
# Team Agent

## Company Style Guide
{{ 'project/styleguide.md' | content }}

## Team Conventions
{{ 'docs/conventions.txt' | content }}
```

**Key Features**:
- 🔒 **Secure**: Path validation prevents traversal attacks
- 📁 **Text files only**: `.md`, `.txt`, `.json`, `.toml`, `.yaml`
- 🔄 **Recursive**: Project files can reference other project files (10-level depth)
- 🎯 **Combine both**: Use dependency content + project files together

See the [Templating Guide](docs/templating.md) for complete documentation and examples.

## Documentation

| Guide | Description |
|-------|-------------|
| [Installation Guide](docs/installation.md) | All installation methods and requirements |
| [User Guide](docs/user-guide.md) | Getting started and basic workflows |
| [Command Reference](docs/command-reference.md) | Complete command syntax and options |
| [Multi-Tool Support](docs/multi-tool-support.md) | Managing resources for multiple AI assistants |
| [Dependencies Guide](docs/dependencies.md) | Version constraints, conflicts, and transitive dependencies |
| [Resources Guide](docs/resources.md) | Working with different resource types |
| [Claude Skills Guide](docs/skills-guide.md) | Working with Claude Skills (NEW!) |
| [Configuration Guide](docs/configuration.md) | Global config, authentication, and patches |
| [Manifest Reference](docs/manifest-reference.md) | Complete agpm.toml schema |
| [Versioning Guide](docs/versioning.md) | Version constraints and Git references |
| [Templating Guide](docs/templating.md) | Dynamic content generation with Tera |
| [Architecture](docs/architecture.md) | Technical details and design decisions |
| [Examples](docs/examples/) | Sample configurations and use cases |
| [FAQ](docs/faq.md) | Frequently asked questions |
| [Troubleshooting](docs/troubleshooting.md) | Common issues and solutions |

## Requirements

- **Rust 1.85.0+** (for building from source)
- **Git 2.0+** (for repository operations)

## Contributing

We welcome contributions! Please see our [Contributing Guide](CONTRIBUTING.md) for details.

## Support

- 🐛 [Issue Tracker](https://github.com/aig787/agpm/issues)
- 💬 [Discussions](https://github.com/aig787/agpm/discussions)
- 📖 [Documentation](docs/user-guide.md)

## License

MIT License - see [LICENSE.md](LICENSE.md) for details.

---

Built with Rust 🦀 for reliability and performance
