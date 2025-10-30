#!/bin/bash
# Example setup script demonstrating AGPM with a local Git repository
# This script sets up a complete Claude Code project with agents, snippets,
# commands, and MCP servers from a local repository

set -e  # Exit on error

# Colors for output
GREEN='\033[0;32m'
BLUE='\033[0;34m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

echo -e "${BLUE}╔════════════════════════════════════════════════════════════════════╗${NC}"
echo -e "${BLUE}║          AGPM Example Project Setup Script                         ║${NC}"
echo -e "${BLUE}╚════════════════════════════════════════════════════════════════════╝${NC}"
echo ""

# Get project name from argument or use default
PROJECT_NAME="${1:-test}"

# Setup paths
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$SCRIPT_DIR/projects/$PROJECT_NAME"
DEPS_DIR="$SCRIPT_DIR/deps"

# Clean up previous example if it exists
echo "→ Cleaning up previous example (if exists)"
rm -rf "$PROJECT_DIR"


# Ensure agpm is built
echo "→ Building agpm"
cd "$(dirname "$SCRIPT_DIR")"
cargo build --release

# Add to PATH for this script
export PATH="$PWD/target/release:$PATH"

echo ""
echo -e "${GREEN}✓ Using agpm from: $(which agpm)${NC}"
echo ""

# Create project directory
echo "→ Creating directory: $PROJECT_DIR"
mkdir -p "$PROJECT_DIR"
cd "$PROJECT_DIR"

# Initialize AGPM manifest
echo "→ Initializing AGPM manifest"
agpm init

# Show initial project structure
echo ""
echo "→ Initial project structure:"
tree -a -L 4

# Add the local-deps source (local directory)
echo ""
echo "→ Adding local-deps source (local directory)"
agpm add source local-deps "$DEPS_DIR"

# Add the agpm-community GitHub repository
echo ""
echo "→ Adding agpm-community GitHub repository"
agpm add source community "https://github.com/aig787/agpm-community.git"

# Add resources with transitive dependencies via commands (using local source)
echo ""
echo -e "${YELLOW}→ Adding commands (which have transitive dependencies)${NC}"
echo "  - git-auto-commit depends on: rust-haiku agent, commit-message snippet"
echo "  - format-json depends on: javascript-haiku agent, data-validation snippet"
agpm add dep command --no-install local-deps:commands/git-auto-commit.md --name git-auto-commit
agpm add dep command --no-install local-deps:commands/format-json.md --name format-json

echo ""
echo -e "${YELLOW}→ Adding agents (which also have dependencies)${NC}"
echo "  - rust-haiku depends on: error-analysis, unit-test-creation snippets"
echo "  - javascript-haiku depends on: test-automation, data-validation snippets"
agpm add dep agent --no-install local-deps:agents/rust-haiku.md --name rust-haiku
agpm add dep agent --no-install local-deps:agents/javascript-haiku.md --name javascript-haiku

echo ""
echo -e "${YELLOW}→ Adding base snippets (dependencies of other resources)${NC}"
agpm add dep snippet --no-install local-deps:snippets/error-analysis.md --name error-analysis
agpm add dep snippet --no-install local-deps:snippets/unit-test-creation.md --name unit-tests
agpm add dep snippet --no-install local-deps:snippets/commit-message.md --name commit-message
agpm add dep snippet --no-install local-deps:snippets/data-validation.md --name data-validation
agpm add dep snippet --no-install local-deps:snippets/test-automation.md --name test-automation

echo ""
echo "→ Adding 2 scripts via command"
agpm add dep script --no-install local-deps:scripts/build.sh --name build
agpm add dep script --no-install local-deps:scripts/test.js --name test

echo ""
echo "→ Adding 2 hooks via command"
agpm add dep hook --no-install local-deps:hooks/pre-tool-use.json --name pre-tool-use
agpm add dep hook --no-install local-deps:hooks/user-prompt-submit.json --name user-prompt-submit

echo ""
echo "→ Adding 2 MCP servers via command"
agpm add dep mcp-server --no-install local-deps:mcp-servers/filesystem.json --name filesystem
agpm add dep mcp-server --no-install local-deps:mcp-servers/fetch.json --name fetch

echo ""
echo "→ Adding skills via command"
agpm add dep skill --no-install local-deps:skills/code-reviewer --name code-reviewer
agpm add dep skill --no-install local-deps:skills/commit-message-generator --name commit-message-generator

echo ""
echo "→ Adding additional agents from agpm-community (without installing)"
agpm add dep agent --no-install --name api-designer "community:agents/awesome-claude-code-subagents/categories/01-core-development/api-designer.md@v0.0.1"
agpm add dep agent --no-install --name backend-developer "community:agents/awesome-claude-code-subagents/categories/01-core-development/backend-developer.md@^v0.0.1"
agpm add dep agent --no-install --name frontend-developer "community:agents/awesome-claude-code-subagents/categories/01-core-development/frontend-developer.md@=v0.0.1"
agpm add dep agent --no-install --name python-pro "community:agents/awesome-claude-code-subagents/categories/02-language-specialists/python-pro.md@v0.0.1"
agpm add dep agent --no-install --name rust-engineer "community:agents/awesome-claude-code-subagents/categories/02-language-specialists/rust-engineer.md@v0.0.1"
agpm add dep agent --no-install --name javascript-pro "community:agents/awesome-claude-code-subagents/categories/02-language-specialists/javascript-pro.md@v0.0.1"
agpm add dep agent --no-install --name database-administrator "community:agents/awesome-claude-code-subagents/categories/03-infrastructure/database-administrator.md@v0.0.1"
agpm add dep agent --no-install --name code-reviewer "community:agents/awesome-claude-code-subagents/categories/04-quality-security/code-reviewer.md@v0.0.1"
agpm add dep agent --no-install --name test-automator "community:agents/awesome-claude-code-subagents/categories/04-quality-security/test-automator.md@v0.0.1"
agpm add dep agent --no-install --name security-auditor "community:agents/awesome-claude-code-subagents/categories/04-quality-security/security-auditor.md@v0.0.1"
agpm add dep agent --no-install --name devops-engineer "community:agents/awesome-claude-code-subagents/categories/03-infrastructure/devops-engineer.md@v0.0.1"
agpm add dep agent --no-install --name cloud-architect "community:agents/awesome-claude-code-subagents/categories/03-infrastructure/cloud-architect.md@v0.0.1"
agpm add dep agent --no-install --name documentation-engineer "community:agents/awesome-claude-code-subagents/categories/06-developer-experience/documentation-engineer.md@v0.0.1"
agpm add dep agent --no-install --name ml-engineer "community:agents/awesome-claude-code-subagents/categories/05-data-ai/ml-engineer.md@v0.0.1"
agpm add dep agent --no-install --name multi-agent-coordinator "community:agents/awesome-claude-code-subagents/categories/09-meta-orchestration/multi-agent-coordinator.md@v0.0.1"

echo ""
echo "→ Adding additional snippets (without installing)"
agpm add dep snippet --no-install --name security-review "local-deps:snippets/security-review.md"
agpm add dep snippet --no-install --name rest-api "local-deps:snippets/rest-api-endpoint.md"
agpm add dep snippet --no-install --name test-coverage "local-deps:snippets/test-coverage.md"


# Add project-level patches to agpm.toml
echo ""
echo -e "${YELLOW}→ Adding project-level patches to agpm.toml${NC}"
echo "  These patches are checked into git and apply to all team members"
cat >> agpm.toml << 'EOF'

# Project-level patches - override resource fields for the whole team
# These are checked into version control and apply to all developers

[patch.agents.rust-haiku]
# Use faster model for quick Rust tasks
model = "haiku"
description = "Rust assistant optimized for speed (team override)"

[patch.agents.javascript-haiku]
# Use faster model for JavaScript tasks
model = "haiku"
description = "JavaScript assistant optimized for speed (team override)"

[patch.agents.security-auditor]
# Security audits need the most capable model
model = "opus"
description = "Security auditor using Opus for thorough analysis (team override)"

[patch.agents.code-reviewer]
# Code reviews benefit from Opus-level reasoning
model = "opus"
EOF

echo ""
echo "→ Generated agpm.toml with project patches:"
cat agpm.toml

# Create private patches in agpm.private.toml
echo ""
echo -e "${YELLOW}→ Creating agpm.private.toml with user-specific patches${NC}"
echo "  These patches are NOT checked into git (add to .gitignore)"
echo "  They extend project patches with personal preferences"
cat > agpm.private.toml << 'EOF'
# Private patches - user-specific overrides
# Add this file to .gitignore - it contains personal preferences
# Different fields combine with project patches; same field causes conflict

[patch.agents.api-designer]
# Personal preference: customize description for my workflow
description = "API designer customized for my microservices architecture"

[patch.agents.documentation-engineer]
# Personal preference: use Opus for comprehensive documentation
model = "opus"

[patch.agents.ml-engineer]
# Personal preference: use Opus for ML work (I work on ML a lot)
model = "opus"
description = "ML engineer using Opus for complex model development"
EOF

echo ""
echo "→ Generated agpm.private.toml:"
cat agpm.private.toml

# Add to .gitignore
echo ""
echo "→ Adding agpm.private.toml to .gitignore"
echo "agpm.private.toml" > .gitignore

# Validate the manifest
echo ""
echo "→ Validating manifest (with patches)"
agpm validate

# Install dependencies
echo ""
echo "→ Installing all dependencies with AGPM"
agpm install

# List installed resources
echo ""
echo "→ Listing installed resources"
agpm list

# Show applied patches in lockfile
echo ""
echo -e "${YELLOW}→ Checking lockfile for applied patches${NC}"
echo "  The lockfile tracks which patches were applied to each resource:"
grep -A 2 "patches =" agpm.lock || echo "  (No patches shown - format may vary)"

# Show a specific patched agent file
echo ""
echo -e "${YELLOW}→ Example: rust-haiku agent with applied patches${NC}"
echo "  Project patches changed model to 'haiku' and updated description:"
if [ -f ".claude/agents/rust-haiku.md" ]; then
    echo ""
    head -n 25 .claude/agents/rust-haiku.md | grep -E "^(model|description):" || echo "  (Fields may not be in frontmatter)"
fi

# Update dependencies
echo ""
echo "→ Updating dependencies with AGPM"
agpm update


echo ""
echo -e "${GREEN}╔════════════════════════════════════════════════════════════════════╗${NC}"
echo -e "${GREEN}║                    Setup Complete! 🎉                              ║${NC}"
echo -e "${GREEN}╚════════════════════════════════════════════════════════════════════╝${NC}"
echo ""
echo "Your Claude Code project '$PROJECT_NAME' is ready:"
echo ""
agpm tree --detailed

# Show final structure
echo ""
echo "→ Final project structure:"
tree -a -L 4

echo ""
echo "Project location: $PROJECT_DIR"
echo ""
echo -e "${BLUE}═══════════════════════════════════════════════════════════════════${NC}"
echo -e "${BLUE}                    Patch System Overview                          ${NC}"
echo -e "${BLUE}═══════════════════════════════════════════════════════════════════${NC}"
echo ""
echo -e "${YELLOW}Project Patches (agpm.toml):${NC}"
echo "  ✓ Checked into git"
echo "  ✓ Apply to all team members"
echo "  ✓ Define team-wide model overrides"
echo "  Examples in this project:"
echo "    - rust-haiku: model='haiku' + custom description"
echo "    - javascript-haiku: model='haiku' + custom description"
echo "    - security-auditor: model='opus' + custom description"
echo "    - code-reviewer: model='opus'"
echo ""
echo -e "${YELLOW}Private Patches (agpm.private.toml):${NC}"
echo "  ✓ NOT checked into git (in .gitignore)"
echo "  ✓ Personal preferences only"
echo "  ✓ Extend project patches with user-specific settings"
echo "  Examples in this project:"
echo "    - api-designer: Custom description for personal workflow"
echo "    - documentation-engineer: model='opus'"
echo "    - ml-engineer: model='opus' + custom description"
echo ""
echo -e "${YELLOW}Patch Merging Rules:${NC}"
echo "  ✓ Different fields: Combine (project + private patches both apply)"
echo "  ✓ Same field: Hard failure (prevents conflicts)"
echo "  ✓ Lockfile: Tracks applied patches in 'patches' field"
echo ""
echo -e "${BLUE}═══════════════════════════════════════════════════════════════════${NC}"
echo ""
echo "To clean up this project, run:"
echo "  ./examples/cleanup_project.sh $PROJECT_NAME"
echo ""
