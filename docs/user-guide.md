# User Guide

This guide covers getting started with AGPM and common workflows. For detailed installation instructions, see the [Installation Guide](installation.md).

## Quick Start

### 1. Initialize a Project

```bash
# Create a new agpm.toml with defaults
agpm init

# Update an existing manifest with latest defaults
agpm init --defaults
```

The `--defaults` flag is useful for updating old manifests to include multi-tool support configurations while preserving your existing settings.

### 2. Install Dependencies

```bash
# Standard installation
agpm install

# CI/production mode (requires exact lockfile match)
agpm install --frozen
```

This clones repositories to `~/.agpm/cache/` and installs resources to your project directories.

### 3. Verify Installation

```bash
agpm list
```

## Basic Concepts

### Manifest File (agpm.toml)

The manifest defines your project's dependencies:

```toml
[sources]
community = "https://github.com/aig787/agpm-community.git"

[agents]
example = { source = "community", path = "agents/example.md", version = "v1.0.0" }

[snippets]
utils = { source = "community", path = "snippets/utils.md", version = "^1.0.0" }

[skills]
rust-helper = { source = "community", path = "skills/rust-helper", version = "v1.0.0" }
```

See the [Manifest Reference](manifest-reference.md) for complete schema documentation or check out [examples](examples/) for more configurations.

### Lockfile (agpm.lock)

The lockfile records exact versions for reproducible installations:
- Generated automatically by `agpm install`
- Should be committed to version control
- Ensures team members get identical versions

#### Lifecycle and Guarantees

- `agpm install` always re-runs dependency resolution using the current manifest and lockfile
- Versions do **not** automatically advance just because you reinstalled
- Use `agpm install --frozen` in CI to ensure exact reproducibility

#### Detecting Staleness

AGPM automatically checks for stale lockfiles:
- Duplicate entries or source URL drift
- Manifest entries missing from the lockfile
- Version/path changes that haven't been resolved

If stale, regenerate with:
```bash
agpm install  # Without --frozen flag
```

### Sources

Sources are Git repositories containing resources:
- Can be public (GitHub, GitLab) or private
- Can be local directories for development
- Authentication handled via global config

See the [Configuration Guide](configuration.md) for setting up private sources.

## Common Workflows

### Adding Dependencies

```bash
# Add via CLI
agpm add dep agent community:agents/helper.md@v1.0.0 --name my-agent

# Or edit agpm.toml and install
agpm install
```

See the [Dependencies Guide](dependencies.md) for:
- Version constraints and patterns
- Transitive dependencies
- Conflict resolution
- Patches and overrides

### Checking for Updates

```bash
# Check all dependencies
agpm outdated

# Check specific ones
agpm outdated my-agent

# CI mode (fails if updates available)
agpm outdated --check
```

### Updating Dependencies

```bash
# Update all within constraints
agpm update

# Update specific dependency
agpm update my-agent

# Preview updates
agpm update --dry-run
```

### Working with Local Resources

For development, use local directories:

```toml
[sources]
local = "./my-resources"

[agents]
dev-agent = { source = "local", path = "agents/dev.md" }
```

Or reference files directly:

```toml
[agents]
local-agent = { path = "../agents/my-agent.md" }
```

### Private Repositories

For private repositories, configure authentication globally:

```bash
# Add private source with token
agpm config add-source private "https://oauth2:TOKEN@github.com/org/private.git"
```

Then reference in your manifest:

```toml
[agents]
internal = { source = "private", path = "agents/internal.md", version = "v1.0.0" }
```

## Team Collaboration

### Setting Up

1. Create and configure `agpm.toml`
2. Run `agpm install` to generate `agpm.lock`
3. Commit both files to Git:

```bash
git add agpm.toml agpm.lock
git commit -m "Add AGPM dependencies"
```

### Team Member Setup

Team members clone the repository and run:

```bash
# Install exact versions from lockfile
agpm install --frozen
```

### Updating Dependencies

When updating dependencies:

1. Update version constraints in `agpm.toml`
2. Run `agpm update`
3. Test the changes
4. Commit the updated `agpm.lock`

## CI/CD Integration

### GitHub Actions

```yaml
- name: Install AGPM
  run: cargo install agpm-cli

- name: Install dependencies
  run: agpm install --frozen
```

### With Authentication

```yaml
- name: Configure AGPM
  run: |
    mkdir -p ~/.agpm
    echo '[sources]' > ~/.agpm/config.toml
    echo 'private = "https://oauth2:${{ secrets.GITHUB_TOKEN }}@github.com/org/private.git"' >> ~/.agpm/config.toml

- name: Install dependencies
  run: agpm install --frozen
```

## Multi-Tool Support

AGPM supports multiple AI coding assistants. By default:
- Snippets install to `.agpm/snippets/` (shared)
- Other resources install for Claude Code

To use resources with different tools:

```toml
[agents]
# Claude Code (default)
helper = { source = "community", path = "agents/helper.md", version = "v1.0.0" }

# OpenCode (explicit)
helper-oc = { source = "community", path = "agents/helper.md", version = "v1.0.0", tool = "opencode" }
```

See the [Multi-Tool Support Guide](multi-tool-support.md) for complete details.

## Pattern Matching

Install multiple resources using glob patterns:

```toml
[agents]
# All AI agents
ai-agents = { source = "community", path = "agents/ai/*.md", version = "v1.0.0" }

# Recursive patterns
all-rust = { source = "community", path = "agents/rust/**/*.md", version = "v1.5.0" }
```

Pattern matching features:
- `*` matches any characters except `/`
- `**` matches any number of directories
- Agents/commands flatten by default (only filename used)
- Snippets/scripts preserve directory structure

See [Dependencies Guide](dependencies.md#pattern-matching) for more details.

## Performance

### Controlling Parallelism

```bash
# Default: max(10, 2 × CPU cores)
agpm install --max-parallel 8

# Single-threaded for debugging
agpm install --max-parallel 1
```

### Cache Management

```bash
# View cache statistics
agpm cache info

# Clean old cache entries
agpm cache clean

# Bypass cache for fresh installation
agpm install --no-cache
```

## Best Practices

1. **Always commit agpm.lock** for reproducible builds
2. **Use semantic versioning** (`v1.0.0`) instead of branches
3. **Validate before committing**: Run `agpm validate`
4. **Use --frozen in production**: `agpm install --frozen`
5. **Keep secrets in global config**, never in `agpm.toml`
6. **Check for outdated dependencies** regularly: `agpm outdated`

## Troubleshooting

### Common Issues

**Manifest not found:**
```bash
agpm init  # Create a new manifest
```

**Version conflicts:**
```bash
agpm validate --resolve  # Check for conflicts
```

**Authentication issues:**
```bash
agpm config list-sources  # Verify source configuration
```

**Lockfile out of sync:**
```bash
agpm install  # Regenerate lockfile
```

### Getting Help

- Run `agpm --help` for command help
- Check the [FAQ](faq.md) for common questions
- See [Troubleshooting Guide](troubleshooting.md) for detailed solutions
- Visit [GitHub Issues](https://github.com/aig787/agpm/issues) for support

## Next Steps

- Explore [available commands](command-reference.md)
- Learn about [resource types](resources.md)
- Understand [versioning](versioning.md)
- Configure [authentication](configuration.md)
- See [example configurations](examples/)