# Resources Guide

CCPM manages six types of resources for Claude Code, divided into two categories based on how they're integrated.

## Resource Categories

### Direct Installation Resources

These resources are copied directly to their target directories and used as standalone files:

- **Agents** - AI assistant configurations
- **Snippets** - Reusable code templates
- **Commands** - Claude Code slash commands
- **Scripts** - Executable automation files

### Configuration-Merged Resources

These resources are installed to `.claude/ccpm/` and then their configurations are merged into Claude Code's settings:

- **Hooks** - Event-based automation
- **MCP Servers** - Model Context Protocol servers

## Resource Types

### Agents

AI assistant configurations with prompts and behavioral definitions.

**Default Location**: `.claude/agents/`

**Example**:
```toml
[agents]
rust-expert = { source = "community", path = "agents/rust-expert.md", version = "v1.0.0" }
local-agent = { path = "../local-agents/helper.md" }  # Direct local path
```

### Snippets

Reusable code templates and documentation fragments.

**Default Location**: `.claude/ccpm/snippets/`

**Example**:
```toml
[snippets]
react-component = { source = "community", path = "snippets/react-component.md", version = "v1.2.0" }
utils = { source = "local-deps", path = "snippets/utils.md" }
```

### Commands

Claude Code slash commands that extend functionality.

**Default Location**: `.claude/commands/`

**Example**:
```toml
[commands]
deploy = { source = "community", path = "commands/deploy.md", version = "v2.0.0" }
lint = { source = "tools", path = "commands/lint.md", branch = "main" }
```

### Scripts

Executable files (.sh, .js, .py, etc.) that can be run by hooks or independently.

**Default Location**: `.claude/ccpm/scripts/`

**Example**:
```toml
[scripts]
security-check = { source = "security-tools", path = "scripts/security.sh", version = "v1.0.0" }
build = { source = "tools", path = "scripts/build.js", version = "v2.0.0" }
validate = { source = "local", path = "scripts/validate.py" }
```

Scripts must be executable and can be written in any language supported by your system.

### Hooks

Event-based automation configurations for Claude Code. JSON files that define when to run scripts.

**Default Location**: `.claude/ccpm/hooks/`
**Configuration**: Automatically merged into `.claude/settings.local.json`

#### Hook Structure

```json
{
  "events": ["PreToolUse"],
  "matcher": "Bash|Write|Edit",
  "type": "command",
  "command": ".claude/ccpm/scripts/security-check.sh",
  "timeout": 5000,
  "description": "Security validation before file operations"
}
```

#### Available Events

- `PreToolUse` - Before a tool is executed
- `PostToolUse` - After a tool completes
- `UserPromptSubmit` - When user submits a prompt
- `UserPromptReceive` - When prompt is received
- `AssistantResponseReceive` - When assistant responds

#### Example Configuration

```toml
[hooks]
pre-bash = { source = "security-tools", path = "hooks/pre-bash.json", version = "v1.0.0" }
file-guard = { source = "security-tools", path = "hooks/file-guard.json", version = "v1.0.0" }
```

### MCP Servers

Model Context Protocol servers that extend Claude Code's capabilities with external tools and APIs.

**Default Location**: `.claude/ccpm/mcp-servers/`
**Configuration**: Automatically merged into `.mcp.json`

#### MCP Server Structure

```json
{
  "command": "npx",
  "args": [
    "-y",
    "@modelcontextprotocol/server-filesystem",
    "--root",
    "./data"
  ],
  "env": {
    "NODE_ENV": "production"
  }
}
```

#### Example Configuration

```toml
[mcp-servers]
filesystem = { source = "community", path = "mcp-servers/filesystem.json", version = "v1.0.0" }
github = { source = "community", path = "mcp-servers/github.json", version = "v1.2.0" }
postgres = { source = "local-deps", path = "mcp-servers/postgres.json" }
```

## Configuration Merging

### How It Works

Configuration-merged resources (Hooks and MCP Servers) follow a two-step process:

1. **File Installation**: JSON configuration files are installed to `.claude/ccpm/`
2. **Configuration Merging**: Settings are automatically merged into Claude Code's configuration files
3. **Non-destructive Updates**: CCPM preserves user-configured entries while managing its own
4. **Tracking**: CCPM adds metadata to track which entries it manages

### Example: Merged .mcp.json

After installation, `.mcp.json` contains both user and CCPM-managed servers:

```json
{
  "mcpServers": {
    "my-manual-server": {
      "command": "node",
      "args": ["./custom.js"]
    },
    "filesystem": {
      "command": "npx",
      "args": [
        "-y",
        "@modelcontextprotocol/server-filesystem",
        "--root",
        "./data"
      ],
      "_ccpm": {
        "managed": true,
        "config_file": ".claude/ccpm/mcp-servers/filesystem.json",
        "installed_at": "2024-01-15T10:30:00Z"
      }
    }
  }
}
```

## File Naming

**Important**: Installed files are named based on the dependency name in `ccpm.toml`, not their original filename.

```toml
[scripts]
# Source file: scripts/build.sh
# Installed as: .claude/ccpm/scripts/my-builder.sh (uses the key "my-builder")
my-builder = { source = "tools", path = "scripts/build.sh" }

[agents]
# Source file: agents/code-reviewer.md
# Installed as: .claude/agents/reviewer.md (uses the key "reviewer")
reviewer = { source = "community", path = "agents/code-reviewer.md" }
```

This allows you to:
- Give resources meaningful names in your project context
- Avoid naming conflicts when using resources from multiple sources
- Rename resources without modifying the source repository

## Custom Installation Paths

Override default installation directories:

```toml
[target]
agents = ".claude/agents"           # Default
snippets = ".claude/ccpm/snippets"  # Default
commands = ".claude/commands"        # Default
scripts = ".claude/ccpm/scripts"    # Default
hooks = ".claude/ccpm/hooks"        # Default
mcp-servers = ".claude/ccpm/mcp-servers"  # Default

# Or use custom paths
agents = "custom/agents"
snippets = "resources/snippets"
```

## Version Control Strategy

By default, CCPM creates `.gitignore` entries to exclude installed files from Git:

- The `ccpm.toml` manifest and `ccpm.lock` lockfile are committed
- Installed resource files are automatically gitignored
- Team members run `ccpm install` to get their own copies

To commit resources to Git instead:

```toml
[target]
gitignore = false  # Don't create .gitignore
```

## Pattern-Based Dependencies

Install multiple resources using glob patterns:

```toml
[agents]
# Install all AI agents
ai-agents = { source = "community", path = "agents/ai/*.md", version = "v1.0.0" }

# Install all review tools recursively
review-tools = { source = "community", path = "agents/**/review*.md", version = "v1.0.0" }

[snippets]
# All Python snippets
python-snippets = { source = "community", path = "snippets/python/*.md", version = "v1.0.0" }
```

## Best Practices

1. **Organize by Function**: Group related resources together
2. **Use Semantic Names**: Choose descriptive names for your dependencies
3. **Version Scripts with Hooks**: Keep scripts and their hook configurations in sync
4. **Test Locally First**: Use local sources during development
5. **Document Requirements**: Note any runtime requirements for scripts/MCP servers
6. **Preserve User Config**: Never manually edit merged configuration files

## Troubleshooting

### Scripts Not Executing

- Ensure scripts have executable permissions
- Check the script path in hook configuration
- Verify required interpreters are installed (bash, python, node, etc.)

### Hooks Not Triggering

- Check `.claude/settings.local.json` for the hook entry
- Verify the event name and matcher pattern
- Check hook timeout settings

### MCP Servers Not Starting

- Ensure required runtimes are installed (Node.js for npx, Python for uvx)
- Check `.mcp.json` for the server configuration
- Verify environment variables are set correctly

### Configuration Not Merging

- Run `ccpm install` again to re-merge configurations
- Check for syntax errors in JSON files
- Ensure CCPM has write permissions to config files