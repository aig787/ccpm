# AGPM Command Reference

This document provides detailed information about all AGPM commands and their options.

## Global Options

```
agpm [OPTIONS] <COMMAND>

Options:
  -v, --verbose              Enable verbose output
  -q, --quiet                Suppress non-error output
      --config <PATH>        Path to custom global configuration file
      --manifest-path <PATH> Path to the manifest file (agpm.toml)
      --no-progress          Disable progress bars and spinners
  -h, --help                 Print help information
  -V, --version              Print version information
```

## Commands

### `agpm init`

Initialize a new AGPM project by creating a `agpm.toml` manifest file, or merge default configurations into an existing manifest.

```bash
agpm init [OPTIONS]

Options:
      --path <PATH>    Initialize in specific directory (default: current directory)
      --force          Overwrite existing agpm.toml file
      --defaults       Merge default configurations into existing manifest
  -h, --help           Print help information
```

**Examples:**
```bash
# Initialize in current directory
agpm init

# Initialize in specific directory
agpm init --path ./my-project

# Force overwrite existing manifest
agpm init --force

# Update existing manifest with default configurations
# (preserves all existing values, comments, and formatting)
agpm init --defaults

# Useful for updating old manifests to include new default sections
# like [tools.claude-code], [tools.opencode], [tools.agpm], etc.
```

**About --defaults flag:**

The `--defaults` flag merges missing default configurations into an existing `agpm.toml` while preserving:
- All existing values and dependencies
- All comments and formatting
- Custom tool configurations

This is useful for:
- Updating old manifests created before multi-tool support
- Adding new default sections (tools, resource types) to existing projects
- Ensuring your manifest has all standard configurations without manual editing

### `agpm install`

Install dependencies from `agpm.toml` and generate/update `agpm.lock`. Automatically updates the lockfile when manifest changes (similar to `cargo build`). Applies patches from both `agpm.toml` and `agpm.private.toml` during installation. Uses centralized version resolution and SHA-based worktree optimization for maximum performance.

```bash
agpm install [OPTIONS]

Options:
      --no-lock                  Don't write lockfile after installation
      --frozen                   Require exact lockfile match (like cargo build --locked)
      --no-cache                 Bypass cache and fetch directly from sources
      --max-parallel <NUMBER>    Maximum parallel operations (default: max(10, 2 × CPU cores))
      --manifest-path <PATH>     Path to agpm.toml (default: ./agpm.toml)
  -h, --help                     Print help information
```

**Examples:**
```bash
# Standard installation (auto-updates lockfile, applies patches)
agpm install

# CI/production mode - fail if lockfile out of sync (like cargo build --locked)
agpm install --frozen

# Install without creating lockfile
agpm install --no-lock

# Bypass cache for fresh fetch
agpm install --no-cache

# Control parallelism (default: max(10, 2 × CPU cores))
agpm install --max-parallel 8

# Use custom manifest path
agpm install --manifest-path ./configs/agpm.toml
```

**Patch Behavior:**
- Reads patches from `[patch.*]` sections in `agpm.toml` (project-level)
- Reads patches from `agpm.private.toml` if present (user-level)
- Private patches extend project patches (different fields combine)
- Conflicts (same field in both) cause installation failure with clear error
- Applied patches are tracked in lockfile `patches` field
- Validates patch aliases match manifest dependencies

### `agpm update`

Update dependencies to latest versions within version constraints. Always regenerates the lockfile with resolved versions.

```bash
agpm update [OPTIONS] [DEPENDENCY]

Arguments:
  [DEPENDENCY]    Update specific dependency (default: update all)

Options:
      --dry-run               Preview changes without applying
      --max-parallel <NUMBER> Maximum parallel operations (default: max(10, 2 × CPU cores))
      --manifest-path <PATH>  Path to agpm.toml (default: ./agpm.toml)
  -h, --help                  Print help information
```

**Examples:**
```bash
# Update all dependencies
agpm update

# Update specific dependency
agpm update rust-expert

# Preview changes
agpm update --dry-run

# Update with custom parallelism
agpm update --max-parallel 6
```

### `agpm outdated`

Check for available updates to installed dependencies. Analyzes the lockfile against available versions in Git repositories to identify dependencies with newer versions available.

```bash
agpm outdated [OPTIONS] [DEPENDENCIES]...

Arguments:
  [DEPENDENCIES]...    Check specific dependencies (default: check all)

Options:
      --format <FORMAT>       Output format: table, json (default: table)
      --check                 Exit with error code 1 if updates are available
      --no-fetch             Use cached repository data without fetching updates
      --max-parallel <NUMBER> Maximum parallel operations (default: max(10, 2 × CPU cores))
      --manifest-path <PATH>  Path to agpm.toml (default: ./agpm.toml)
      --no-progress          Disable progress bars and spinners
  -h, --help                  Print help information
```

**Examples:**
```bash
# Check all dependencies for updates
agpm outdated

# Check specific dependencies
agpm outdated rust-expert my-agent

# Use in CI - exit with error if outdated
agpm outdated --check

# Use cached data without fetching
agpm outdated --no-fetch

# JSON output for scripting
agpm outdated --format json

# Control parallelism
agpm outdated --max-parallel 5
```

**Output Information:**

The command displays:
- **Current**: The version currently installed (from lockfile)
- **Latest**: The newest version that satisfies the manifest's version constraint
- **Available**: The absolute newest version available in the repository
- **Type**: The resource type (agent, snippet, command, script, hook, mcp-server)

**Version Analysis:**

The outdated command performs sophisticated version comparison:
1. **Compatible Updates**: Versions that satisfy the current version constraint in agpm.toml
2. **Major Updates**: Newer versions that exceed the constraint (require manual manifest update)
3. **Up-to-date**: Dependencies already on the latest compatible version

**JSON Output Format:**

When using `--format json`, the output includes:
```json
{
  "outdated": [
    {
      "name": "my-agent",
      "type": "agent",
      "source": "community",
      "current": "v1.0.0",
      "latest": "v1.2.0",
      "latest_available": "v2.1.0",
      "constraint": "^1.0.0",
      "has_update": true,
      "has_major_update": true
    }
  ],
  "summary": {
    "total": 5,
    "outdated": 2,
    "with_updates": 1,
    "with_major_updates": 1,
    "up_to_date": 3
  }
}
```

### `agpm list`

List installed resources from `agpm.lock`. Shows "(patched)" indicator for resources with applied patches.

```bash
agpm list [OPTIONS]

Options:
      --format <FORMAT>       Output format: table, json (default: table)
      --type <TYPE>           Filter by resource type: agents, snippets, commands, scripts, hooks, mcp-servers, skills
      --manifest-path <PATH>  Path to agpm.toml (default: ./agpm.toml)
  -h, --help                  Print help information
```

**Examples:**
```bash
# List all resources in table format
agpm list

# List only agents
agpm list --type agents

# List only skills
agpm list --type skills

# Output as JSON (includes patch field names)
agpm list --format json

# Use custom manifest path
agpm list --manifest-path ./configs/agpm.toml
```

**Output Details:**

Table format shows "(patched)" indicator:
```text
Name          Type    Version  Source     Installed At                    Status
rust-expert   agent   v1.0.0   community  .claude/agents/rust-expert.md   (patched)
helper        agent   v1.0.0   community  .claude/agents/helper.md
```

JSON format includes patch field names:
```json
{
  "agents": [
    {
      "name": "rust-expert",
      "version": "v1.0.0",
      "source": "community",
      "patches": ["model", "temperature", "max_tokens"]
    }
  ]
}
```

### `agpm tree`

Display dependency trees for installed resources with transitive dependencies. Visualizes the complete dependency graph similar to `cargo tree`, helping identify duplicate or redundant dependencies.

```bash
agpm tree [OPTIONS]

Options:
  -d, --depth <NUMBER>        Maximum depth to display (unlimited if not specified)
  -f, --format <FORMAT>       Output format: tree, json, text [default: tree]
  -p, --package <NAME>        Show tree for specific package only
      --duplicates            Show only duplicate dependencies
      --no-dedupe             Don't deduplicate repeated dependencies
      --agents                Show only agents
      --snippets              Show only snippets
      --commands              Show only commands
      --scripts               Show only scripts
      --hooks                 Show only hooks
      --mcp-servers           Show only MCP servers
  -i, --invert                Invert tree to show what depends on each package
      --manifest-path <PATH>  Path to agpm.toml (default: ./agpm.toml)
  -h, --help                  Print help information
```

**Examples:**
```bash
# Display full dependency tree
agpm tree

# Limit tree depth to 2 levels
agpm tree --depth 2

# Show tree for specific package
agpm tree --package my-agent

# Show only duplicate dependencies
agpm tree --duplicates

# JSON output for scripting
agpm tree --format json

# Show only agents and their dependencies
agpm tree --agents

# Invert tree to see what depends on each package
agpm tree --invert

# Show tree without deduplication
agpm tree --no-dedupe
```

**Output Format:**

The tree format displays dependencies hierarchically with these elements:
- Package name with type prefix (agent/, snippet/, command/, etc.)
- Version information
- Source repository in parentheses
- `(*)` marker indicates duplicate dependency (shown once by default)

**Example Tree Output:**
```text
my-project
├── agent/code-reviewer v1.0.0 (community)
│   ├── agent/rust-helper v1.0.0 (community)
│   └── snippet/utils v2.1.0 (community)
├── command/git-commit v1.0.0 (local)
│   ├── agent/rust-helper v1.0.0 (community) (*)
│   └── snippet/commit-msg v1.0.0 (local)
└── snippet/logging v1.5.0 (community)

(*) = duplicate dependency
```

**JSON Format:**

Use `--format json` for programmatic access to dependency information, which includes complete metadata about each dependency and its relationships.

### `agpm validate`

Validate `agpm.toml` syntax, dependency resolution, patch configuration, template rendering, and file references. Also validates `agpm.private.toml` if present.

```bash
agpm validate [OPTIONS]

Options:
      --check-lock            Also validate lockfile consistency
      --resolve               Perform full dependency resolution
      --render                Validate template rendering and file references
      --sources               Check if all sources are accessible
      --paths                 Check if local file paths exist
      --format <FORMAT>       Output format: text or json (default: text)
      --strict                Treat warnings as errors
      --quiet                 Suppress informational messages
      --verbose               Enable verbose output
      --manifest-path <PATH>  Path to agpm.toml (default: ./agpm.toml)
  -h, --help                  Print help information
```

**Examples:**
```bash
# Basic syntax validation (includes patch validation)
agpm validate

# Validate with lockfile consistency check
agpm validate --check-lock

# Full validation with dependency resolution
agpm validate --resolve

# Validate template rendering and file references
agpm validate --render

# Comprehensive validation for CI/CD
agpm validate --resolve --check-lock --render --strict

# JSON output for automation
agpm validate --format json

# Validate custom manifest
agpm validate --manifest-path ./configs/agpm.toml
```

**Validation Checks:**

**Basic Manifest Validation** (always performed):
- TOML syntax correctness
- Source and dependency definitions
- Patch syntax and structure
- Patch aliases match manifest dependencies
- No unknown patch references

**Lockfile Validation** (`--check-lock`):
- Lockfile exists and is valid
- Lockfile matches manifest (no staleness)
- All dependencies are present
- Applied patches tracked correctly

**Dependency Resolution** (`--resolve`):
- Full dependency resolution
- Version constraint satisfaction
- Transitive dependency resolution
- Patch conflict detection between project and private patches

**Template and File Reference Validation** (`--render`):
- **Template Rendering**: Validates that all markdown resources with template syntax can be successfully rendered
  - Checks `{{`, `{%`, `{#` template syntax
  - Validates template variables and context
  - Reports rendering errors with file location
- **File Reference Auditing**: Checks that all file references within markdown content point to existing files
  - Validates markdown links: `[text](path.md)`
  - Validates direct file paths: `.agpm/snippets/file.md`, `docs/guide.md`
  - Ignores URLs (http://, https://), code blocks (```), and absolute paths
  - Reports broken references with clear error messages

**Source Accessibility** (`--sources`):
- Tests network connectivity to all source repositories
- Verifies credentials and access permissions

**Path Validation** (`--paths`):
- Checks that local file dependencies exist on filesystem
- Validates relative paths are within project boundaries

### `agpm add`

Add sources or dependencies to `agpm.toml`.

#### Add Source

```bash
agpm add source <NAME> <URL> [OPTIONS]

Arguments:
  <NAME>    Source name
  <URL>     Git repository URL or local path

Options:
      --manifest-path <PATH>  Path to agpm.toml (default: ./agpm.toml)
  -h, --help                  Print help information
```

#### Add Dependency

```bash
agpm add dep <RESOURCE_TYPE> <SPEC> [OPTIONS]

Arguments:
  <RESOURCE_TYPE>  Resource type: agent, snippet, command, script, hook, mcp-server, skill
  <SPEC>           Dependency specification (see formats below)

Options:
      --name <NAME>           Dependency name (default: derived from path)
      --tool <TOOL>           Target tool: claude-code, opencode, agpm, custom
      --target <PATH>         Custom installation path (relative to resource directory)
      --filename <NAME>       Custom filename for the installed resource
  -f, --force                 Force overwrite if dependency exists
      --no-install            Add to manifest without installing (install later with 'agpm install')
      --manifest-path <PATH>  Path to agpm.toml (default: ./agpm.toml)
  -h, --help                  Print help information
```

**Dependency Specification Formats:**

The `<SPEC>` argument supports multiple formats for different source types:

1. **Git Repository Dependencies** - `source:path[@version]`
   - `source`: Name of a Git source defined in `[sources]` section
   - `path`: Path to file(s) within the repository
   - `version`: Optional Git ref (tag/branch/commit), defaults to "main"

2. **Local File Dependencies** - Direct file paths
   - Absolute paths: `/home/user/agents/local.md`, `C:\Users\name\agent.md`
   - Relative paths: `./agents/local.md`, `../shared/snippet.md`
   - File URLs: `file:///home/user/script.sh`

3. **Pattern Dependencies** - Using glob patterns
   - `source:agents/*.md@v1.0.0` - All .md files in agents directory
   - `source:snippets/**/*.md` - All .md files recursively
   - `./local/**/*.json` - All JSON files from local directory

**Examples:**
```bash
# Add a source repository first
agpm add source community https://github.com/aig787/agpm-community.git

# Git repository dependencies
agpm add dep agent community:agents/rust-expert.md@v1.0.0
agpm add dep agent community:agents/rust-expert.md  # Uses "main" branch
agpm add dep snippet community:snippets/react.md@feature-branch

# Local file dependencies
agpm add dep agent ./local-agents/helper.md --name my-helper
agpm add dep script /usr/local/scripts/build.sh
agpm add dep hook ../shared/hooks/pre-commit.json
agpm add dep skill ../my-skills/rust-helper --name rust-helper

# Pattern dependencies (bulk installation)
agpm add dep agent "community:agents/ai/*.md@v1.0.0" --name ai-agents
agpm add dep snippet "community:snippets/**/*.md" --name all-snippets
agpm add dep script "./scripts/*.sh" --name local-scripts
agpm add dep skill "community:skills/*/*.md@v1.0.0" --name all-skills

# Windows paths
agpm add dep agent C:\Resources\agents\windows.md
agpm add dep script "file://C:/Users/name/scripts/build.ps1"

# Custom names (recommended for patterns)
agpm add dep agent community:agents/reviewer.md --name code-reviewer
agpm add dep snippet "community:snippets/python/*.md" --name python-utils

# Force overwrite existing dependency
agpm add dep agent community:agents/new-version.md --name existing-agent --force

# Add multiple dependencies without installing (batch mode)
agpm add dep agent --no-install community:agents/rust-expert.md@v1.0.0 --name rust-expert
agpm add dep agent --no-install community:agents/python-pro.md@v1.0.0 --name python-pro
agpm add dep snippet --no-install community:snippets/utils.md@v1.0.0 --name utils
# Then install all at once
agpm install

# Specify target tool for multi-tool projects
agpm add dep agent community:agents/helper.md@v1.0.0 --tool opencode --name opencode-helper
agpm add dep agent community:agents/helper.md@v1.0.0 --tool claude-code --name claude-helper
```

**Name Derivation:**

If `--name` is not provided, the dependency name is automatically derived from the file path:
- `agents/reviewer.md` → name: "reviewer"
- `snippets/utils.md` → name: "utils"
- `/path/to/helper.md` → name: "helper"

For pattern dependencies, you should typically provide a custom name since multiple files will be installed.

See the [Manifest Reference](manifest-reference.md) for inline table fields (`branch`, `rev`, `target`, `filename`, MCP settings) and advanced configuration after the dependency is added.

### `agpm remove`

Remove sources or dependencies from `agpm.toml`.

#### Remove Source

```bash
agpm remove source <NAME> [OPTIONS]

Arguments:
  <NAME>    Source name to remove

Options:
      --manifest-path <PATH>  Path to agpm.toml (default: ./agpm.toml)
  -h, --help                  Print help information
```

#### Remove Dependency

```bash
agpm remove dep <RESOURCE_TYPE> <NAME> [OPTIONS]

Arguments:
  <RESOURCE_TYPE>  Resource type: agent, snippet, command, script, hook, mcp-server, skill
  <NAME>           Dependency name to remove

Options:
      --manifest-path <PATH>  Path to agpm.toml (default: ./agpm.toml)
  -h, --help                  Print help information
```

**Examples:**
```bash
# Remove a source
agpm remove source old-repo

# Remove an agent
agpm remove dep agent old-agent

# Remove a snippet
agpm remove dep snippet unused-snippet

# Remove a skill
agpm remove dep skill old-skill
```

### `agpm config`

Manage global configuration in `~/.agpm/config.toml`.

#### Show Configuration

```bash
agpm config show [OPTIONS]

Options:
      --no-mask    Show actual token values (use with caution)
  -h, --help       Print help information
```

#### Initialize Configuration

```bash
agpm config init [OPTIONS]

Options:
      --force      Overwrite existing configuration
  -h, --help       Print help information
```

#### Edit Configuration

```bash
agpm config edit [OPTIONS]

Options:
  -h, --help    Print help information
```

#### Manage Sources

```bash
# Add source with authentication
agpm config add-source <NAME> <URL>

# List all global sources (tokens masked)
agpm config list-sources

# Remove source
agpm config remove-source <NAME>
```

**Examples:**
```bash
# Show current configuration (tokens masked)
agpm config show

# Initialize config with examples
agpm config init

# Edit config in default editor
agpm config edit

# Add private source with token
agpm config add-source private "https://oauth2:ghp_xxxx@github.com/org/private.git"

# List all sources
agpm config list-sources

# Remove a source
agpm config remove-source old-private
```

### `agpm upgrade`

Self-update AGPM to the latest version or a specific version. Includes automatic backup and rollback capabilities with built-in security features.

```bash
agpm upgrade [OPTIONS] [VERSION]

Arguments:
  [VERSION]    Target version to upgrade to (e.g., "0.3.18" or "v0.3.18")

Options:
      --check       Check for updates without installing
  -s, --status      Show current version and latest available
  -f, --force       Force upgrade even if already on latest version
      --rollback    Rollback to previous version from backup
      --no-backup   Skip creating a backup before upgrade
  -h, --help        Print help information
```

**Examples:**
```bash
# Upgrade to latest version
agpm upgrade

# Check for available updates
agpm upgrade --check

# Show current and latest version
agpm upgrade --status

# Upgrade to specific version
agpm upgrade 0.3.18

# Force reinstall latest version
agpm upgrade --force

# Rollback to previous version
agpm upgrade --rollback

# Upgrade without creating backup
agpm upgrade --no-backup
```

#### Security Features

The upgrade command implements multiple security measures to ensure safe updates:

- **GitHub Integration**: Only downloads binaries from official AGPM GitHub releases
- **HTTPS Downloads**: Uses secure HTTPS connections for all network operations
- **Platform-Specific Archives**: Downloads appropriate archive format for your platform (.tar.xz for Unix, .zip for Windows)
- **Atomic Operations**: Minimizes vulnerability windows during binary replacement
- **Permission Preservation**: Maintains original file permissions and ownership
- **Backup Protection**: Creates backups with appropriate permissions before any modifications

### `agpm cache`

Manage the global Git repository cache in `~/.agpm/cache/`. The cache uses SHA-based worktrees for optimal deduplication and performance.

#### Cache Information

```bash
agpm cache info [OPTIONS]

Options:
  -h, --help    Print help information
```

#### Clean Cache

```bash
agpm cache clean [OPTIONS]

Options:
      --all       Remove all cached repositories
      --unused    Remove unused repositories only (default)
  -h, --help      Print help information
```

**Examples:**
```bash
# Show cache statistics
agpm cache info

# Clean unused repositories
agpm cache clean

# Remove all cached repositories
agpm cache clean --all
```

### `agpm migrate`

Migrate from legacy CCPM naming to AGPM. This is a one-time migration command for projects upgrading from the legacy CCPM naming scheme.

```bash
agpm migrate [OPTIONS]

Options:
  -p, --path <PATH>    Path to directory containing ccpm.toml/ccpm.lock (default: current directory)
      --dry-run        Show what would be renamed without actually renaming files
  -h, --help           Print help information
```

**Examples:**
```bash
# Migrate in current directory
agpm migrate

# Migrate with custom path
agpm migrate --path /path/to/project

# Dry run to preview changes
agpm migrate --dry-run
```

**Behavior:**
- Detects `ccpm.toml` and `ccpm.lock` files in the specified directory
- Renames them to `agpm.toml` and `agpm.lock` respectively
- Fails with an error if target files already exist (conflict detection)
- Provides clear feedback and next steps after migration

## Resource Types

AGPM manages six types of resources with optimized parallel installation:

### Direct Installation Resources

- **Agents**: AI assistant configurations (installed to `.claude/agents/`)
- **Snippets**: Reusable code templates (installed to `.agpm/snippets/` by default)
- **Commands**: Claude Code slash commands (installed to `.claude/commands/`)
- **Scripts**: Executable automation files (installed to `.claude/scripts/`)

### Configuration-Merged Resources

- **Hooks**: Event-based automation (merged into `.claude/settings.local.json`)
- **MCP Servers**: Model Context Protocol servers (merged into `.mcp.json`)

### Parallel Installation Features

- **Worktree-based processing**: Each resource uses an isolated Git worktree for safe concurrent installation
- **Configurable concurrency**: Use `--max-parallel` to control the number of simultaneous operations
- **Real-time progress**: Multi-phase progress tracking shows installation status across all parallel operations
- **Instance-level optimization**: Worktrees are cached and reused within a single command for maximum efficiency

## Version Constraints

AGPM supports semantic version constraints:

| Syntax | Description | Example |
|--------|-------------|---------|
| `1.0.0` | Exact version | `version = "1.0.0"` |
| `^1.0.0` | Compatible releases | `version = "^1.0.0"` (>=1.0.0, <2.0.0) |
| `~1.0.0` | Patch releases only | `version = "~1.0.0"` (>=1.0.0, <1.1.0) |
| `>=1.0.0` | Minimum version | `version = ">=1.0.0"` |
| `latest` | Latest stable tag | `version = "latest"` |
| `*` | Any version | `version = "*"` |

## Git References

Alternative to semantic versions:

- **Branches**: `branch = "main"` (mutable, updates on install)
- **Commits**: `rev = "abc123"` (immutable, exact commit)
- **Local paths**: No versioning, uses current files

## Pattern Dependencies

Use glob patterns to install multiple resources:

```toml
[agents]
# Install all AI agents
ai-tools = { source = "community", path = "agents/ai/*.md", version = "v1.0.0" }

# Install all review tools recursively
review-tools = { source = "community", path = "agents/**/review*.md", version = "v1.0.0" }
```

## Parallelism Control

AGPM v0.3.0 introduces advanced parallelism control for optimal performance:

### --max-parallel Flag

Available on `install` and `update` commands to control concurrent operations:

- **Default**: `max(10, 2 × CPU cores)` - Automatically scales with system capacity
- **Range**: 1 to 100 parallel operations
- **Use Cases**:
  - High-performance systems: Increase for faster operations
  - Limited bandwidth: Reduce to avoid overwhelming network
  - CI/CD environments: Tune based on available resources

**Examples:**
```bash
# Use default parallelism (recommended)
agpm install

# High-performance system with fast network
agpm install --max-parallel 20

# Limited bandwidth or shared resources
agpm install --max-parallel 3

# Single-threaded operation (debugging)
agpm install --max-parallel 1
```

### Performance Characteristics

- **Worktree-Based**: Uses Git worktrees for parallel-safe repository access
- **Instance Caching**: Per-command fetch cache reduces redundant network operations
- **Smart Batching**: Dependencies from same source share operations where possible
- **Memory Efficient**: Each parallel operation uses minimal memory overhead

## Environment Variables

AGPM respects these environment variables:

- `AGPM_CONFIG` - Path to custom global config file
- `AGPM_CACHE_DIR` - Override cache directory
- `AGPM_NO_PROGRESS` - Disable progress bars
- `AGPM_MAX_PARALLEL` - Default parallelism level (overridden by --max-parallel flag)
- `RUST_LOG` - Set logging level (debug, info, warn, error)

## Exit Codes

AGPM uses these exit codes:

- `0` - Success
- `1` - General error
- `2` - Invalid arguments or command usage
- `3` - Manifest validation error
- `4` - Dependency resolution error
- `5` - Git operation error
- `6` - File I/O error
- `101` - Panic or critical error

## Configuration Examples

### Basic Project

```toml
# agpm.toml
[sources]
community = "https://github.com/aig787/agpm-community.git"

[agents]
rust-expert = { source = "community", path = "agents/rust-expert.md", version = "v1.0.0" }

[snippets]
react-hooks = { source = "community", path = "snippets/react-hooks.md", version = "^1.0.0" }
```

### Advanced Project

```toml
# agpm.toml
[sources]
community = "https://github.com/aig787/agpm-community.git"
tools = "https://github.com/myorg/agpm-tools.git"
local = "./local-resources"

[agents]
# Pattern-based dependency
ai-agents = { source = "community", path = "agents/ai/*.md", version = "v1.0.0" }
# Single file dependency
custom-agent = { source = "tools", path = "agents/custom.md", version = "^2.0.0" }

[snippets]
python-utils = { source = "community", path = "snippets/python/*.md", version = "~1.2.0" }

[commands]
deploy = { source = "tools", path = "commands/deploy.md", branch = "main" }

[scripts]
build = { source = "local", path = "scripts/build.sh" }

[hooks]
pre-commit = { source = "community", path = "hooks/pre-commit.json", version = "v1.0.0" }

[mcp-servers]
filesystem = { source = "community", path = "mcp/filesystem.json", version = "latest" }

[target]
# Custom installation paths
agents = "custom/agents"
snippets = "resources/snippets"
# Disable gitignore generation
gitignore = false
```

## Getting Help

- Run `agpm --help` for general help
- Run `agpm <command> --help` for command-specific help
- Check the [FAQ](docs/faq.md) for common questions
- Visit [GitHub Issues](https://github.com/aig787/agpm/issues) for support
