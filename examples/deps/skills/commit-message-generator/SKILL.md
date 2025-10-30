---
name: commit-message-generator
description: Generate conventional commit messages based on git diff analysis. Use when you need to create well-structured commit messages following conventional commit format.
---

# Commit Message Generator

## Instructions

When generating commit messages, follow these guidelines:

### 1. Analyze the Git Diff
- Examine all staged and unstaged changes
- Identify the main purpose of the changes
- Look for breaking changes, new features, or bug fixes

### 2. Choose the Appropriate Type
Use one of these conventional commit types:
- `feat`: New feature or enhancement
- `fix`: Bug fix or regression
- `docs`: Documentation changes only
- `style`: Code style changes (formatting, missing semicolons, etc.)
- `refactor`: Code refactoring without functional changes
- `test`: Adding or updating tests
- `chore`: Maintenance tasks, dependency updates, etc.
- `perf`: Performance improvements
- `ci`: CI/CD configuration changes
- `build`: Build system or dependency changes

### 3. Format the Commit Message
```
<type>[optional scope]: <description>

[optional body]

[optional footer(s)]
```

#### Rules:
- Keep the description under 72 characters
- Use imperative mood ("add" not "added" or "adds")
- Include scope if the change affects a specific module/component
- Add "BREAKING CHANGE:" footer for breaking API changes
- Reference issues with "Closes #123" or "Resolves #456"

### 4. Examples

#### Simple Feature
```
feat(cli): add --verbose flag for detailed output

Adds a new verbose flag that displays detailed progress information
during installation and update operations.
```

#### Bug Fix
```
fix(cache): resolve worktree cleanup issue on Windows

The worktree cleanup logic was failing on Windows due to path
separator mismatches. This fix ensures proper path normalization.

Closes #123
```

#### Breaking Change
```
feat(api): change dependency resolution return type

The resolve_dependencies function now returns a Result<Dependencies>
instead of (Dependencies, Warnings) for better error handling.

BREAKING CHANGE: This changes the function signature and requires
callers to handle the Result type.
```

### 5. Special Cases
- For multiple unrelated changes, create multiple commits
- For WIP (work in progress), use "WIP:" prefix
- For revert commits, use "revert: <original commit message>"
- For merge commits, use "Merge branch 'feature-branch'"

## Usage

1. Stage your changes with `git add`
2. Run this skill to analyze the changes
3. Review and edit the generated commit message if needed
4. Commit with `git commit -m "<message>"`

## Tips

- Focus on what the change does, not how it was implemented
- Be specific but concise in the description
- Consider future readers when writing the message
- Link to related documentation or issues when relevant