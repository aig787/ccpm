# Manifest Reference

This guide summarizes every field that appears in `agpm.toml` and how CLI inputs map to the manifest schema. Use it alongside the command reference when editing manifests manually or generating them with `agpm add`.

## Manifest Layout

```toml
[sources]                 # Named Git or local repositories
[project]                 # Optional: Project-specific template variables for AI agents
[default-tools]           # Optional: Override default tool for resource types
[tools.claude-code]       # Optional: Configure Claude Code tool
[tools.opencode]          # Optional: Configure OpenCode tool
[tools.agpm]              # Optional: Configure AGPM tool
[agents]                  # Resource sections share the same dependency schema
[snippets]
[commands]
[scripts]
[hooks]
[mcp-servers]
[skills]                  # Directory-based resources with SKILL.md
[patch.<type>.<name>]     # Optional: Override resource fields
```

Each resource table maps a dependency name (key) to either a simple string path or an inline table with detailed settings.

## Dependency Forms

| Form | When to use | Example | Manifest shape |
| --- | --- | --- | --- |
| Simple path | Local files with no extra metadata | `helper = "../shared/helper.md"` | `ResourceDependency::Simple` |
| Detailed table | Remote Git resources, patterns, custom install behavior | `ai-helper = { source = "community", path = "agents/helper.md", version = "^1.0" }` | `ResourceDependency::Detailed` |

### Detailed Dependency Fields

| Field | Required | Applies to | Description | CLI mapping |
| --- | --- | --- | --- | --- |
| `source` | Only for Git resources | agents/snippets/commands/scripts/hooks/mcp-servers/skills | Name from `[sources]`; omit for local filesystem paths. | Parsed from the `source:` prefix (e.g., `community:...`). |
| `path` | Yes | All | File path inside the repo (Git) or filesystem path/glob (local). Patterns are detected by `*`, `?`, or `[]`. | Parsed from the middle portion of the spec. |
| `version` | Default `"main"` for Git | Git resources | Tag, semantic range, `latest`, or branch alias. Used when no explicit `branch`/`rev` are provided. | Parsed from `@value` when using `agpm add dep`. Defaults to `main` if omitted. |
| `tool` | Default varies by resource | All | Target tool: `claude-code`, `opencode`, `agpm`, or custom. **Defaults**: snippets → `agpm`, all others → `claude-code`. Routes resources to tool-specific directories. | Manual edit. |
| `branch` | No | Git resources | Track a branch tip. Overrides `version` when present. Requires manual manifest edit today. | Add manually: `{ branch = "develop" }`. |
| `rev` | No | Git resources | Exact commit SHA (short or full). Highest precedence when set. | Add manually; not provided by current CLI shorthand. |
| `command` | MCP servers | MCP | Launch command (e.g., `npx`, `uvx`). | Use inline table or edit manifest. |
| `args` | MCP servers | MCP | Command arguments array. | Manual edit. |
| `target` | Optional | All | Override install subdirectory relative to artifact base directory. | Manual edit. |
| `filename` | Optional | All | Force output filename (with extension). | Manual edit. |
| `dependencies` | Auto-generated | All | Extracted transitive dependencies from resource metadata. Do not edit by hand. | Populated during install. |

> **Priority rules**: `rev` (commit) overrides `branch`, which overrides `version`. If you set multiple selectors, AGPM picks the most specific one.

### CLI Spec → Manifest Examples

```text
community:agents/reviewer.md@v1.0.0   → { source = "community", path = "agents/reviewer.md", version = "v1.0.0" }
community:agents/reviewer.md          → { source = "community", path = "agents/reviewer.md", version = "main" }
./local/agent.md --name helper        → helper = "./local/agent.md"
```

To track a branch or commit, edit the manifest entry manually:

```toml
[agents]
nightly = { source = "community", path = "agents/dev.md", branch = "main" }
pinned  = { source = "community", path = "agents/dev.md", rev = "abc123def" }
```

## Pattern Dependencies

- Specify glob characters (`*`, `?`, `[]`, `**`) in `path` to install multiple files.
- Provide a descriptive dependency name (`ai-agents`, `all-snippets`) so lockfile entries are easy to read.
- AGPM expands the pattern during install and records every concrete match in `agpm.lock` under the resolved dependency, using `resource_type/name@resolved_version` entries.
- Conflicts are detected after expansion—if two patterns resolve to the same install location, the install fails with a duplicate-path error (see the conflicts section for remediation guidance).

## Transitive Dependencies

Resources can declare their own dependencies within their content using YAML frontmatter (for Markdown files) or JSON fields (for JSON files). AGPM automatically resolves these transitive dependencies during installation, creating a complete dependency graph.

### Declaration Syntax

**Markdown files** (YAML frontmatter):

```yaml
---
dependencies:
  agents:
    - path: agents/helper.md
      version: v1.0.0        # Optional: inherits parent's version if not specified
      tool: claude-code      # Optional: inherits parent's tool if not specified
  snippets:
    - path: snippets/utils.md
---
# Your resource content here...
```

**JSON files** (top-level field):

```json
{
  "dependencies": {
    "commands": [
      {
        "path": "commands/deploy.md",
        "version": "v2.0.0",
        "tool": "opencode"
      }
    ]
  },
  "mcpServers": {
    "myserver": {
      "command": "npx",
      "args": ["-y", "myserver"]
    }
  }
}
```

### Dependency Fields

- **`path`** (required): Path to the dependency file within the same source repository. **Supports templates** using `{{ agpm.project.* }}` variables
- **`version`** (optional): Version constraint (e.g., `v1.0.0`, `^v2.1.0`). If omitted, inherits from parent resource
- **`tool`** (optional): Target tool (`claude-code`, `opencode`, `agpm`). If omitted, inherits from parent if compatible, otherwise uses resource type default

### Templated Dependency Paths

Dependency paths support template variables from the `[project]` section, enabling dynamic dependency resolution based on project configuration.

**Example** - Language-specific dependencies:

```yaml
---
dependencies:
  snippets:
    - path: snippets/standards/{{ agpm.project.language }}-guide.md
      version: v1.0.0
  commands:
    - path: commands/{{ agpm.project.framework }}/deploy.md
---
```

With project configuration:
```toml
[project]
language = "rust"
framework = "tokio"
```

Resolves to:
- `snippets/standards/rust-guide.md`
- `commands/tokio/deploy.md`

**Features**:
- Uses same `agpm.project.*` context as content templating
- Supports the `default` filter for optional variables: `{{ agpm.project.env | default(value="dev") }}`
- Respects per-resource `agpm.templating: false` opt-out
- Errors on undefined variables (use `default` filter for optional variables)

See [Templating Guide](templating.md#templating-dependency-paths) for more details and examples.

### Resolution Rules

**Same-Source Model**: Transitive dependencies must come from the same source repository as their parent. This ensures version consistency and simplifies dependency management.

**Version Inheritance**: If a transitive dependency doesn't specify a version, it inherits the parent resource's version. This allows dependency trees to stay synchronized across releases.

```toml
# agpm.toml
[agents]
ai-assistant = { source = "community", path = "agents/assistant.md", version = "v2.0.0" }
```

If `agents/assistant.md` declares dependencies without versions:
```yaml
---
dependencies:
  snippets:
    - path: snippets/prompts.md  # Inherits v2.0.0
    - path: snippets/utils.md    # Inherits v2.0.0
---
```

**Tool Inheritance**: Tools are inherited when the parent's tool supports the child's resource type. For example, if a `claude-code` agent depends on a snippet:
- If parent explicitly sets `tool: claude-code` and `claude-code` supports snippets → inherits
- If not compatible → falls back to resource type's default tool

### Graph-Based Resolution

AGPM uses a dependency graph with topological ordering to resolve transitive dependencies:

1. **Collection**: Gathers all direct dependencies from manifest
2. **Expansion**: Extracts transitive dependencies from each resource's metadata
3. **Graph Building**: Constructs dependency graph with cycle detection
4. **Topological Sort**: Orders dependencies so each resource is installed after its dependencies
5. **Parallel Resolution**: Resolves independent branches concurrently for maximum efficiency

**Cycle Detection**: If a circular dependency is detected (A → B → C → A), installation fails with a clear error:

```text
Error: Circular dependency detected
  agents/a.md → agents/b.md → agents/c.md → agents/a.md
```

### Deduplication

If multiple resources depend on the same file, AGPM automatically deduplicates:

```toml
[agents]
agent-a = { source = "community", path = "agents/a.md", version = "v1.0.0" }
agent-b = { source = "community", path = "agents/b.md", version = "v1.0.0" }
```

If both `a.md` and `b.md` declare:
```yaml
dependencies:
  snippets:
    - path: snippets/shared.md
```

Result: `snippets/shared.md` is installed once and shared by both agents.

### Conflict Resolution

If the same resource is needed at different versions, AGPM selects the highest version that satisfies all constraints:

- `agents/a.md` requires `snippets/helper.md` at `^v1.0.0`
- `agents/b.md` requires `snippets/helper.md` at `^v1.2.0`
- **Resolution**: Installs `v1.2.0` (satisfies both `^v1.0.0` and `^v1.2.0`)

If constraints are incompatible, installation fails with a version conflict error.

### Viewing the Dependency Tree

Use `agpm tree` to visualize the complete dependency graph:

```bash
agpm tree
```

Output shows transitive relationships:
```
agents/assistant (v2.0.0)
├─ snippets/prompts (v2.0.0) [transitive]
└─ snippets/utils (v2.0.0) [transitive]
    └─ snippets/common (v2.0.0) [transitive]
```

See `agpm tree --help` for filtering options.

## Naming Overrides

| Setting | Section | Purpose | Example |
| --- | --- | --- | --- |
| `target` field | Dependency table | Move a single resource | `tool = { ..., target = "custom/tools" }` |
| `filename` field | Dependency table | Override installed filename | `tool = { ..., filename = "dev-tool.md" }` |

## Tool Configuration

AGPM supports multiple AI coding assistants through configurable tools. Each tool defines where resources are installed.

> ⚠️ **Alpha Feature**: OpenCode support is currently in alpha. While functional, it may have incomplete features or breaking
> changes in future releases. Claude Code support is stable and production-ready.

### Default Tools

| Tool | Base Directory | Supported Resources | Status |
| --- | --- | --- | --- |
| `claude-code` (default) | `.claude` | agents, commands, scripts, hooks, mcp-servers, snippets | ✅ Stable |
| `opencode` | `.opencode` | agents, commands, mcp-servers | 🚧 Alpha |
| `agpm` | `.agpm` | snippets | ✅ Stable |

### Using the Tool Field

Add `tool` to any dependency to route it to a specific tool:

```toml
[agents]
# Default: installs to .claude/agents/helper.md (agents default to claude-code)
claude-agent = { source = "community", path = "agents/helper.md", version = "v1.0.0" }

# OpenCode: installs to .opencode/agent/helper.md (note: singular "agent") - Alpha
opencode-agent = { source = "community", path = "agents/helper.md", version = "v1.0.0", tool = "opencode" }

[snippets]
# Default: snippets install to .agpm/snippets/ (snippets default to agpm, not claude-code)
shared = { source = "community", path = "snippets/rust-patterns.md", version = "v1.0.0" }

# Claude Code specific: explicitly set tool to install to .claude/snippets/
claude-specific = { source = "community", path = "snippets/claude.md", version = "v1.0.0", tool = "claude-code" }
```

### Custom Tool Configuration

Override default directories or define custom tools:

```toml
[tools.claude-code]
path = ".claude"
resources = { 
  agents = { path = "agents" }, 
  commands = { path = "commands" },
  hooks = { merge-target = ".claude/settings.local.json" },
  mcp-servers = { merge-target = ".mcp.json" }
}

[tools.opencode]
path = ".opencode"
resources = { 
  agents = { path = "agent" }, 
  commands = { path = "command" },
  mcp-servers = { merge-target = ".opencode/opencode.json" }
}

[tools.custom-tool]
path = ".mytool"
resources = { 
  agents = { path = "agents" }, 
  commands = { path = "cmds" },
  hooks = { merge-target = ".mytool/hooks.json" },
  mcp-servers = { merge-target = ".mytool/servers.json" }
}
```

**Important**: Resource types that merge into configuration files (hooks, mcp-servers) must specify `merge-target` (with a hyphen). Resource types that install as files (agents, snippets, commands, scripts) must specify `path`.

### MCP Server Configuration

MCP servers automatically route to the correct configuration file based on tool:

```toml
[mcp-servers]
# Merges into .mcp.json
claude-fs = { source = "community", path = "mcp/filesystem.json", version = "v1.0.0" }

# Merges into opencode.json - Alpha
opencode-fs = { source = "community", path = "mcp/filesystem.json", version = "v1.0.0", tool = "opencode" }
```

### Merge Targets

Some resource types (hooks, MCP servers) don't install as individual files but merge into shared configuration files. The `merge-target` field in tool resource configuration specifies these merge destinations.

**Default Merge Targets**:
- **Hooks** (claude-code): `.claude/settings.local.json`
- **MCP Servers** (claude-code): `.mcp.json`
- **MCP Servers** (opencode): `.opencode/opencode.json`

**Custom Merge Targets**:

You can override merge targets for custom tools or alternative configurations:

```toml
# Define custom tool with custom merge target
[tools.my-tool]
path = ".my-tool"

[tools.my-tool.resources.hooks]
merge-target = ".my-tool/hooks.json"

[tools.my-tool.resources.mcp-servers]
merge-target = ".my-tool/servers.json"
```

**Path vs. Merge Target**:

- **`path`**: Used for file-based resources (agents, snippets, commands, scripts) that install as individual `.md`, `.sh`, `.js`, or `.py` files in subdirectories
- **`merge-target`**: Used for configuration-based resources (hooks, MCP servers) that merge into shared JSON configuration files
- A resource type is supported if **either** `path` OR `merge-target` is specified

**Note**: Custom tools require MCP handlers for hooks/MCP servers. Only built-in tools (claude-code, opencode) have handlers. Custom merge targets work best by overriding defaults for built-in tools rather than creating wholly custom tools.

## Default Tools Configuration

The `[default-tools]` section allows you to override which tool is used by default for each resource type when a dependency doesn't explicitly specify a `tool` field.

### Built-in Defaults

When not configured, AGPM uses these defaults:
- `snippets` → `agpm` (shared infrastructure)
- All other resources → `claude-code`

### Configuration Syntax

```toml
[default-tools]
snippets = "claude-code"  # Override default for Claude-only users
agents = "claude-code"    # Explicit (already the default)
commands = "opencode"     # Default to OpenCode for commands
```

### Supported Keys

You can configure defaults for any resource type:

| Key | Description | Built-in Default |
| --- | --- | --- |
| `agents` | Default tool for agent resources | `claude-code` |
| `snippets` | Default tool for snippet resources | `agpm` |
| `commands` | Default tool for command resources | `claude-code` |
| `scripts` | Default tool for script resources | `claude-code` |
| `hooks` | Default tool for hook resources | `claude-code` |
| `mcp-servers` | Default tool for MCP server resources | `claude-code` |

### Behavior

**Default Application**:
```toml
[default-tools]
agents = "opencode"

[agents]
# Uses default: installs to .opencode/agent/
helper = { source = "community", path = "agents/helper.md", version = "v1.0.0" }
```

**Explicit Override**:
```toml
[default-tools]
agents = "opencode"  # Default for agents

[agents]
# Uses default: .opencode/agent/
default-agent = { source = "community", path = "agents/helper.md", version = "v1.0.0" }

# Explicit tool overrides default: .claude/agents/
claude-agent = { source = "community", path = "agents/helper.md", version = "v1.0.0", tool = "claude-code" }
```

### Use Cases

**Claude Code Only Users**:
```toml
[default-tools]
snippets = "claude-code"  # Install snippets to .claude/snippets/ instead of .agpm/snippets/
```

**OpenCode Preferred**:
```toml
[default-tools]
agents = "opencode"
commands = "opencode"
```

**Mixed Workflows**:
```toml
[default-tools]
snippets = "agpm"        # Shared snippets (explicit, already default)
agents = "claude-code"   # Claude Code agents
commands = "opencode"    # OpenCode commands
```

## Project Variables

The `[project]` section defines arbitrary template variables that AI agents can reference when generating or reviewing code. This provides project-specific context about conventions, documentation locations, and coding standards.

### Structure

The `[project]` section has **no predefined structure** - you can organize variables however makes sense for your project. All TOML types are supported: strings, numbers, booleans, arrays, and nested tables.

```toml
[project]
# Arbitrary variables - organize however you want
style_guide = "docs/STYLE_GUIDE.md"
max_line_length = 100
test_framework = "pytest"
require_tests = true

# Nested sections for organization (optional)
[project.paths]
architecture = "docs/ARCHITECTURE.md"
conventions = "docs/CONVENTIONS.md"
test_data = "tests/fixtures"

[project.standards]
indent_style = "spaces"
indent_size = 4
naming_convention = "snake_case"

[project.team]
backend_lead = "alice"
frontend_lead = "bob"
deployment_envs = ["staging", "production"]
```

### Template Access

All project variables are accessible in templates under the `agpm.project` namespace:

```markdown
---
name: code-reviewer
---
# Code Reviewer

Follow our style guide: {{ agpm.project.style_guide }}

## Standards
- Max line length: {{ agpm.project.max_line_length }}
- Indentation: {{ agpm.project.standards.indent_size }} {{ agpm.project.standards.indent_style }}

## Documentation
Refer to:
- Architecture: {{ agpm.project.paths.architecture }}
- Test data: {{ agpm.project.paths.test_data }}

{% if agpm.project.require_tests %}
All code changes must include tests using {{ agpm.project.test_framework }}.
{% endif %}
```

### Use Cases

**For AI Agents:**
- Reference project-specific style guides and conventions
- Access documentation paths without hardcoding
- Understand testing requirements and frameworks
- Adapt to project-specific naming conventions

**For Teams:**
- Standardize AI agent guidance across the team
- Version-control agent context alongside code
- Share project conventions in a machine-readable format

### Key Features

- **Completely flexible** - No required fields, any TOML structure works
- **Nested sections** - Use dotted paths for organization
- **All types supported** - Strings, numbers, booleans, arrays, tables
- **Optional** - Templates work without the `[project]` section
- **Template-only** - Project variables are only available in templates, not used by AGPM itself

See the [Templating Guide](templating.md#project-variables) for more examples and template syntax.

## Patches and Overrides

Override YAML frontmatter or JSON fields in resource files without forking upstream repositories. Patches enable customization of model settings, temperature, API keys, and any other metadata field.

### Syntax

```toml
[patch.<resource-type>.<alias>]
field = "value"
nested.field = "value"
```

- `<resource-type>`: The resource section (agents, snippets, commands, scripts, hooks, mcp-servers)
- `<alias>`: Must match a dependency name from the corresponding resource section
- Fields support all TOML types: strings, numbers, booleans, arrays, tables

### Project-Level Patches (agpm.toml)

Committed to version control. Applied to all users of the project:

```toml
[agents]
rust-expert = { source = "community", path = "agents/rust-expert.md", version = "v1.0.0" }
ai-assistant = { source = "community", path = "agents/ai/assistant.md", version = "v1.0.0" }

[patch.agents.rust-expert]
model = "claude-3-haiku"       # Override model
temperature = "0.8"            # Adjust temperature
max_tokens = "4096"            # Set token limit

[patch.agents.ai-assistant]
system_prompt = "You are a helpful AI assistant focused on clarity."
tools = ["web-search", "calculator"]
```

### Private Patches (agpm.private.toml)

User-level overrides, never committed (add to .gitignore). Private patches **extend** project patches:

```toml
# agpm.private.toml
[patch.agents.rust-expert]
api_key = "${MY_ANTHROPIC_KEY}"          # Personal API key
custom_endpoint = "https://my-proxy.internal"
logging_level = "debug"

# Different fields from project patch - no conflict!
# Project patch: model, temperature, max_tokens
# Private patch: api_key, custom_endpoint, logging_level
```

### Conflict Behavior

When the **same field** appears in both project and private patches, installation fails with a clear error:

```text
Error: Patch conflict for agents/rust-expert
  Field 'model' appears in both agpm.toml and agpm.private.toml
  Resolution: Keep the field in one file only
```

**When fields differ**, patches merge successfully:
- Project patch: `model`, `temperature`
- Private patch: `api_key`, `logging_level`
- Result: All four fields applied

### Supported Resource Types

Patches work with all resource types:

```toml
[patch.agents.my-agent]
model = "claude-3-haiku"

[patch.snippets.python-utils]
author = "Internal Team"
version = "2.0.0"

[patch.commands.deploy]
timeout = "300"
retry_count = "3"

[patch.scripts.build]
shell = "/bin/bash"
environment.NODE_ENV = "production"

[patch.hooks.pre-commit]
enabled = false

[patch.mcp-servers.filesystem]
args = ["--root", "/custom/path"]
```

### Pattern Dependencies

Patches require explicit dependency names. For pattern dependencies, you must reference individual resolved files:

```toml
# Pattern dependency
[agents]
ai-agents = { source = "community", path = "agents/ai/*.md", version = "v1.0.0" }

# After installation, check `agpm list` to see resolved names
# Then patch specific files that matched the pattern:
[patch.agents.ai-assistant]
model = "claude-3-haiku"

[patch.agents.ai-analyzer]
model = "claude-3-opus"
```

### Validation

Patches are validated during installation:

```bash
# Check for unknown dependency aliases
agpm validate

# Full validation including patch application
agpm install
```

**Validation rules:**
1. Patch alias must exist in the corresponding resource section
2. Cannot patch dependencies that don't exist in manifest
3. Conflicting fields between project and private patches cause hard failure
4. All TOML syntax must be valid

### Lockfile Tracking

Applied patches are tracked in `agpm.lock` for reproducibility:

```toml
[[agents]]
name = "rust-expert"
source = "community"
path = "agents/rust-expert.md"
version = "v1.0.0"
resolved_commit = "abc123..."
checksum = "sha256:..."
installed_at = ".claude/agents/rust-expert.md"
patches = ["model", "temperature", "max_tokens", "api_key"]
```

The `patches` field lists all applied patch fields (from both project and private patches).

### Viewing Patched Resources

Use `agpm list` to identify patched resources:

```bash
$ agpm list
Name          Type    Version  Source     Installed At                    Status
rust-expert   agent   v1.0.0   community  .claude/agents/rust-expert.md   (patched)
ai-assistant  agent   v1.0.0   community  .claude/agents/ai-assistant.md  (patched)
helper        agent   v1.0.0   community  .claude/agents/helper.md
```

Detailed view shows patch fields:

```bash
$ agpm list --format json
{
  "agents": [
    {
      "name": "rust-expert",
      "version": "v1.0.0",
      "patches": ["model", "temperature", "max_tokens", "api_key"]
    }
  ]
}
```

### Use Cases

**Team Configuration** (agpm.toml):
```toml
[patch.agents.code-reviewer]
model = "claude-3-opus"        # Team standard model
guidelines_url = "https://internal.wiki/code-review"
```

**Personal Customization** (agpm.private.toml):
```toml
[patch.agents.code-reviewer]
temperature = "0.9"            # Personal preference
api_key = "${ANTHROPIC_KEY}"  # Personal credentials
```

**Development vs Production**:
```toml
# Development agpm.toml
[patch.agents.deployer]
environment = "development"
dry_run = true

# Production agpm.toml
[patch.agents.deployer]
environment = "production"
dry_run = false
```

## Recommended Workflow

1. Use `agpm add dep` for initial entries—this ensures naming and defaults are correct.
2. Edit the generated inline table when you need advanced selectors (`branch`, `rev`, `tool`), custom install paths, or MCP launch commands.
3. Add `[patch]` sections to customize resource metadata without forking.
4. Use `agpm.private.toml` for personal overrides (API keys, preferences).
5. Re-run `agpm install` (or `agpm validate --resolve`) after manual edits to confirm the manifest parses and resolves correctly.
