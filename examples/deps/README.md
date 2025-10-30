# AGPM Example Resources

This directory contains example resources (agents, commands, snippets, etc.) that can be used with AGPM.

## Content Embedding Feature (v0.4.8+)

AGPM supports embedding content from other files into your resources via two mechanisms:

### 1. Dependency Content Embedding (Versioned)

Resources can declare dependencies in their frontmatter and access the content of those dependencies in templates. This is useful for sharing versioned content across multiple resources.

**Example:**

```markdown
---
agpm.templating: true
dependencies:
  snippets:
    - path: snippets/rust-patterns.md
      name: rust_patterns
      version: v1.0.0
---
## Shared Patterns

{{ agpm.deps.snippets.rust_patterns.content }}
```

**Key Features:**
- Content is versioned (pulled from git at specified version)
- Markdown frontmatter is automatically stripped
- JSON metadata fields are removed
- Content is cached and reused across resources
- Set `install: false` to only make content available in templates without creating a file

### 2. Content Filter (Project-Local)

The `content` filter reads files from your project directory at install time. This is useful for embedding project-specific documentation or configuration.

**Example:**

```markdown
---
agpm.templating: true
---
## Project Style Guide

{{ 'docs/style-guide.md' | content }}
```

**Key Features:**
- Reads from project directory (where agpm.toml is located)
- Path validation prevents directory traversal
- Supports text files: .md, .txt, .json, .toml, .yaml
- Markdown frontmatter is stripped
- JSON is pretty-printed
- Maximum 10 levels of recursive inclusion
- Maximum 1MB file size

## Example Resources

### Claude Code Skills

These skills demonstrate various use cases for Claude Code's skill system. Skills are directories containing a `SKILL.md` file with YAML frontmatter and markdown instructions.

#### Available Skills

- **commit-message-generator** - Generates conventional commit messages based on git diff analysis
  - **Features**:
    - Follows conventional commit format (feat, fix, docs, etc.)
    - Handles breaking changes and issue references
    - Includes scope and detailed descriptions
  - **Resources**:
    - `scripts/commit-analyzer.py` - Automated commit message analysis tool
    - `templates/commit-template.md` - Commit message template with guidelines
    - `examples.md` - Detailed examples for different scenarios

- **code-reviewer** - Performs comprehensive code reviews
  - **Features**:
    - Checks correctness, performance, security, and maintainability
    - Provides structured feedback format
    - Includes review checklist and best practices
  - **Resources**:
    - `scripts/review-analyzer.py` - Automated code review analysis tool
    - `templates/review-template.md` - Structured review template
    - `examples.md` - Review examples and integration guides

- **csv-data-auditor** - Validates and audits CSV data quality
  - **Features**:
    - Checks for missing values, duplicates, and outliers
    - Validates data types and consistency
    - Generates detailed audit reports with recommendations
  - **Resources**:
    - `scripts/csv_validator.py` - Comprehensive CSV validation tool
    - `templates/validation-rules.json` - Business rules template
    - `examples.md` - Validation scenarios and batch processing examples

- **pdf-processor** - Processes PDF files for various tasks
  - **Features**:
    - Text extraction with multiple methods
    - Form filling and field detection
    - Table extraction and OCR support
    - PDF manipulation (merge, split, watermark)
  - **Resources**:
    - `scripts/pdf_extractor.py` - Full-featured PDF processing tool
    - `templates/form-data-template.json` - Form field templates
    - `examples.md` - Comprehensive usage examples for different scenarios

#### Skill Structure

Each skill follows the Claude Code Skills directory structure:
```
skill-name/
├── SKILL.md                 # Main skill definition (required)
├── scripts/                 # Executable scripts and utilities
│   ├── *.py                # Python tools
│   ├── *.js                # JavaScript utilities
│   └── *.sh                # Shell scripts
├── templates/               # Template files and configurations
│   ├── *.md                # Markdown templates
│   ├── *.json              # Configuration templates
│   └── *.txt               # Text templates
└── examples.md              # Detailed usage examples
```

#### Using Skills

Skills are automatically available in Claude Code when placed in:
- Project skills: `.claude/skills/` (shared via git)
- Personal skills: `~/.claude/skills/` (local only)

Each skill contains:
- **SKILL.md**: Main instructions for Claude to follow
- **Scripts**: Automated tools for common tasks
- **Templates**: Reusable configurations and formats
- **Examples**: Detailed usage scenarios and code samples

### Agents with Content Embedding

- **code-reviewer-with-standards.md** - Demonstrates dependency content embedding with multiple snippets for coding standards and best practices

### Supporting Snippets

These snippets are designed to be used as dependencies:

- **rust-patterns.md** - Common Rust patterns and idioms
- **code-quality-checklist.md** - Quality checklist for code reviews
- **api-design-principles.md** - API design best practices

## Using These Examples

1. Add the local source to your agpm.toml:
   ```toml
   [sources]
   examples = "/path/to/agpm/examples/deps"
   ```

2. Install an example resource:
   ```toml
   [agents]
   code-reviewer = { source = "examples", path = "agents/code-reviewer-with-standards.md" }
   ```

3. Create a project-local file for the content filter (if using that feature):
   ```bash
   mkdir -p docs
   echo "# Project Style Guide" > docs/style-guide.md
   ```

4. Run `agpm install` to install the resources

## See Also

- [Content Filter Documentation](../../docs/content-filter.md)
- [Templating Documentation](../../docs/templating.md)
- [Dependency System](../../docs/dependencies.md)
