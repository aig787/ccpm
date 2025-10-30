# Conventional Commit Template

## Type Options
- **feat**: New feature or enhancement
- **fix**: Bug fix or regression
- **docs**: Documentation changes only
- **style**: Code style changes (formatting, missing semicolons, etc.)
- **refactor**: Code refactoring without functional changes
- **test**: Adding or updating tests
- **chore**: Maintenance tasks, dependency updates, etc.
- **perf**: Performance improvements
- **ci**: CI/CD configuration changes
- **build**: Build system or dependency changes

## Format
```
<type>[optional scope]: <description>

[optional body]

[optional footer(s)]
```

## Rules
- Keep description under 72 characters
- Use imperative mood ("add" not "added" or "adds")
- Include scope if change affects specific module
- Add "BREAKING CHANGE:" footer for breaking API changes
- Reference issues with "Closes #123" or "Resolves #456"