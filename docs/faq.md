# CCPM Frequently Asked Questions

## Table of Contents

- [General Questions](#general-questions)
- [Installation & Setup](#installation--setup)
- [Dependencies & Versions](#dependencies--versions)
- [Resource Management](#resource-management)
- [Version Control](#version-control)
- [Team Collaboration](#team-collaboration)
- [Troubleshooting](#troubleshooting)
- [Advanced Usage](#advanced-usage)

## General Questions

### What is CCPM?
CCPM (Claude Code Package Manager) is a Git-based package manager for Claude Code resources. It enables reproducible installations of AI agents, snippets, commands, scripts, hooks, and MCP servers from Git repositories using a lockfile-based approach similar to Cargo or npm.

### How does CCPM differ from other package managers?
Unlike traditional package managers with central registries, CCPM is fully decentralized and Git-based. Resources are distributed directly from Git repositories, and versioning is tied to Git tags, branches, or commits. This makes it perfect for managing AI-related resources that may be proprietary or experimental.

### What types of resources can CCPM manage?
CCPM manages six resource types:
- **Direct Installation**: Agents, Snippets, Commands, Scripts (copied directly to target directories)
- **Configuration-Merged**: Hooks, MCP Servers (installed then merged into Claude Code config files)

### Do I need Git installed to use CCPM?
Yes, CCPM uses your system's Git command for all repository operations. This ensures maximum compatibility and respects your existing Git configuration (SSH keys, credentials, etc.).

## Installation & Setup

### How do I install CCPM?

CCPM is published to crates.io with automated releases. You can install via:

**All Platforms (via Cargo):**
```bash
# Requires Rust toolchain
cargo install ccpm  # Published to crates.io
# Or from GitHub for latest development
cargo install --git https://github.com/aig787/ccpm.git
```

**Unix/macOS (Pre-built Binary):**
```bash
# Download pre-built binary (automatically detects architecture)
curl -L https://github.com/aig787/ccpm/releases/latest/download/ccpm-$(uname -m)-$(uname -s | tr '[:upper:]' '[:lower:]').tar.gz | tar xz
chmod +x ccpm
sudo mv ccpm /usr/local/bin/
```

**Windows (Pre-built Binary):**
```powershell
# PowerShell
Invoke-WebRequest -Uri "https://github.com/aig787/ccpm/releases/latest/download/ccpm-x86_64-windows.zip" -OutFile ccpm.zip
Expand-Archive -Path ccpm.zip -DestinationPath .
# Move to a directory in PATH or add to PATH manually
Move-Item ccpm.exe "$env:LOCALAPPDATA\ccpm\bin\"
```

### How do I start a new CCPM project?
```bash
ccpm init  # Creates ccpm.toml
# or
ccpm init --path ./my-project  # Creates in specific directory
```

### What's the difference between ccpm.toml and ccpm.lock?
- **ccpm.toml**: Your project manifest that declares dependencies and their version constraints (you edit this)
- **ccpm.lock**: Auto-generated file with exact resolved versions for reproducible builds (don't edit manually)

## Dependencies & Versions

### Can I use specific versions of resources?
Yes! CCPM supports multiple versioning strategies:
```toml
exact = { source = "repo", path = "file.md", version = "v1.0.0" }      # Exact tag
range = { source = "repo", path = "file.md", version = "^1.0.0" }      # Compatible versions
branch = { source = "repo", path = "file.md", branch = "main" }        # Track branch
commit = { source = "repo", path = "file.md", rev = "abc123" }         # Specific commit
```

### What version constraints are supported?
CCPM uses semantic versioning constraints like Cargo:
- `^1.2.3` - Compatible updates (>=1.2.3, <2.0.0)
- `~1.2.3` - Patch updates only (>=1.2.3, <1.3.0)
- `>=1.0.0, <2.0.0` - Custom ranges
- `latest` - Latest stable tag
- `*` - Any version

### How do I update dependencies?
```bash
ccpm update              # Update all to latest compatible versions
ccpm update agent-name   # Update specific dependency
ccpm update --dry-run    # Preview changes without applying
```

### Can I use local files without Git?
Yes! You can reference local directories or individual files:
```toml
[sources]
local = "./local-resources"  # Local directory (no Git required)

[agents]
from-dir = { source = "local", path = "agents/helper.md" }  # From local source
direct = { path = "../agents/my-agent.md" }                 # Direct file path
```

## Resource Management

### Where are resources installed?
Default installation directories:
- Agents: `.claude/agents/`
- Snippets: `.claude/ccpm/snippets/`
- Commands: `.claude/commands/`
- Scripts: `.claude/ccpm/scripts/`
- Hooks: `.claude/ccpm/hooks/` (then merged into `.claude/settings.local.json`)
- MCP Servers: `.claude/ccpm/mcp-servers/` (then merged into `.mcp.json`)

### Can I customize installation directories?
Yes, there are two ways to customize where resources are installed:

**1. Global defaults** - Use the `[target]` section in ccpm.toml:
```toml
[target]
agents = "custom/agents/path"
snippets = "custom/snippets/path"
commands = "custom/commands/path"
```

**2. Per-dependency override** - Use the `target` attribute on individual dependencies:
```toml
[agents]
# Uses default from [target] or built-in default
standard-agent = { source = "repo", path = "agents/standard.md" }

# Override installation path for this specific agent
special-agent = { source = "repo", path = "agents/special.md", target = "special/location/agent.md" }
```

The per-dependency `target` takes precedence over global `[target]` settings.

### How are installed files named?
Files are named based on the dependency key in ccpm.toml, not their source filename:
```toml
[agents]
my-helper = { source = "repo", path = "agents/assistant.md" }
# Installs as: .claude/agents/my-helper.md (not assistant.md)
```

### What's the difference between hooks and scripts?
- **Scripts**: Executable files (.sh, .js, .py) that perform tasks
- **Hooks**: JSON configurations that define when to run scripts based on Claude Code events

### How do hooks and MCP servers get configured?
These are "configuration-merged" resources:
1. JSON files are installed to `.claude/ccpm/`
2. Configurations are automatically merged into Claude Code's settings
3. User-configured entries are preserved

## Version Control

### What should I commit to Git?
Commit these files:
- `ccpm.toml` - Your dependency manifest
- `ccpm.lock` - Locked versions for reproducible builds

Don't commit:
- `.claude/` directory (auto-generated, gitignored by default)
- `~/.ccpm/config.toml` (contains secrets)

### Why are my installed files gitignored?
By default, CCPM creates `.gitignore` entries to prevent installed dependencies from being committed. This follows the pattern of other package managers where you commit the manifest but not the installed packages.

### Can I commit installed resources to Git?
Yes, set `gitignore = false` in ccpm.toml:
```toml
[target]
gitignore = false  # Don't create .gitignore
```

### How does CCPM handle the .gitignore file?
CCPM manages a section in `.gitignore` marked with "CCPM managed entries". It preserves any user entries outside this section while updating its own entries based on installed resources.

## Team Collaboration

### How do team members get the same versions?
1. Commit both `ccpm.toml` and `ccpm.lock` to your repository
2. Team members run `ccpm install --frozen` to install exact lockfile versions
3. This ensures everyone has identical resource versions

### How do I handle private repositories?
For repositories requiring authentication:
```bash
# Add to global config (not committed)
ccpm config add-source private "https://oauth2:TOKEN@github.com/org/private.git"

# Or use SSH in ccpm.toml (safe to commit)
[sources]
private = "git@github.com:org/private.git"
```

### What's the difference between global and local sources?
- **Global sources** (`~/.ccpm/config.toml`): For credentials and private repos, not committed
- **Local sources** (`ccpm.toml`): Project-specific sources, safe to commit

Sources are resolved with global sources first, then local sources can override.

### What's the --frozen flag for?
`ccpm install --frozen` uses exact versions from ccpm.lock without checking for updates. Use this in CI/CD and production environments for deterministic builds.

## Troubleshooting

### Installation fails with "No manifest found"
Create a ccpm.toml file:
```bash
ccpm init  # Creates ccpm.toml
```

### How do I debug installation issues?
```bash
# Run with debug logging
RUST_LOG=debug ccpm install

# Validate manifest and sources
ccpm validate --resolve

# Check cache status
ccpm cache info
```

### Resources aren't being installed
1. Check ccpm.toml syntax: `ccpm validate`
2. Verify source repositories are accessible
3. Check Git authentication for private repos
4. Clear cache if corrupted: `ccpm cache clean --all`

### How do I handle version conflicts?
```bash
# Check for conflicts
ccpm validate --resolve

# Update constraints in ccpm.toml
# Then regenerate lockfile
ccpm install
```

### Can I uninstall resources?
CCPM doesn't have an uninstall command. To remove resources:
1. Remove the dependency from ccpm.toml
2. Run `ccpm install` to update
3. Manually delete the installed files if needed

### What if my existing Claude Code settings conflict?
CCPM preserves user-configured settings in:
- `.claude/settings.local.json` (for hooks)
- `.mcp.json` (for MCP servers)

Only CCPM-managed entries (marked with metadata) are updated.

## Advanced Usage

### Can I use multiple sources for redundancy?
Yes, you can define multiple sources and use different ones for different dependencies:
```toml
[sources]
primary = "https://github.com/org/resources.git"
backup = "https://gitlab.com/org/resources.git"
local = "./local-resources"
```

### How do I bypass the cache?
```bash
ccpm install --no-cache  # Fetch directly from sources
```

### Can I control parallel downloads?
```bash
ccpm install --max-parallel 4  # Limit concurrent operations
```

### How do I clean up the cache?
```bash
ccpm cache clean       # Remove unused repositories
ccpm cache clean --all # Clear entire cache
ccpm cache info        # View cache statistics
```

### Can I reference resources from subdirectories?
Yes, use the path field to specify subdirectories:
```toml
[agents]
nested = { source = "repo", path = "deep/path/to/agent.md" }
```

### How do environment variables work in configurations?
MCP server and hook configurations support `${VAR}` expansion:
```json
{
  "command": "node",
  "env": {
    "API_KEY": "${MY_API_KEY}"
  }
}
```

### Can I use CCPM in CI/CD pipelines?
Yes! Best practices for CI/CD:
1. Commit ccpm.lock to your repository
2. Use `ccpm install --frozen` in CI
3. Set authentication in environment or global config
4. Use `--max-parallel` to control resource usage

### What platforms does CCPM support?
CCPM is tested and supported on:
- macOS (x86_64, aarch64)
- Linux (x86_64, aarch64)
- Windows (x86_64)

### Where can I find example resources?
Check out community repositories:
- [ccpm-community](https://github.com/aig787/ccpm-community) - Official community resources
- Search GitHub for repositories with "ccpm" topics

## Still Have Questions?

If your question isn't answered here:
1. Check the [full documentation](README.md)
2. Search [existing issues](https://github.com/aig787/ccpm/issues)
3. Ask in [GitHub Discussions](https://github.com/aig787/ccpm/discussions)
4. Report bugs via [GitHub Issues](https://github.com/aig787/ccpm/issues/new)