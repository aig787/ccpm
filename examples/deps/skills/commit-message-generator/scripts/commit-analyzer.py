#!/usr/bin/env python3
"""
Commit Message Analyzer
Analyzes git diff and suggests conventional commit messages
"""

import subprocess
import re
import sys
from pathlib import Path
from typing import List, Dict, Tuple

def run_git_command(cmd: List[str]) -> str:
    """Run git command and return output"""
    try:
        result = subprocess.run(
            ['git'] + cmd,
            capture_output=True,
            text=True,
            check=True
        )
        return result.stdout.strip()
    except subprocess.CalledProcessError as e:
        print(f"Error running git command: {e}", file=sys.stderr)
        return ""

def get_staged_files() -> List[str]:
    """Get list of staged files"""
    output = run_git_command(['diff', '--cached', '--name-only'])
    return output.split('\n') if output else []

def get_unstaged_files() -> List[str]:
    """Get list of unstaged modified files"""
    output = run_git_command(['diff', '--name-only'])
    return output.split('\n') if output else []

def get_diff_summary(files: List[str], staged: bool = True) -> str:
    """Get diff summary for specific files"""
    if not files:
        return ""

    cmd = ['diff', '--stat']
    if staged:
        cmd.insert(1, '--cached')
    cmd.extend(files)

    return run_git_command(cmd)

def analyze_file_changes(diff_output: str) -> Dict[str, int]:
    """Analyze diff output to categorize changes"""
    categories = {
        'feat': 0,
        'fix': 0,
        'docs': 0,
        'style': 0,
        'refactor': 0,
        'test': 0,
        'chore': 0,
        'perf': 0,
        'ci': 0,
        'build': 0
    }

    # Patterns for different types of changes
    patterns = {
        'feat': [
            r'new.*function',
            r'add.*feature',
            r'implement',
            r'\+def.*new',
            r'\+class.*\w+',
            r'\+async.*def'
        ],
        'fix': [
            r'bug.*fix',
            r'fix.*issue',
            r'resolve.*error',
            r'correct.*logic',
            r'patch.*bug',
            r'-.*\w*error'
        ],
        'docs': [
            r'\.md',
            r'readme',
            r'documentation',
            r'comment.*update',
            r'docstring'
        ],
        'style': [
            r'format',
            r'lint',
            r'indent',
            r'whitespace',
            r'semicolon',
            r'coding.*style'
        ],
        'refactor': [
            r'refactor',
            r'restructure',
            r'reorganize',
            r'extract.*method',
            r'rename.*\w+',
            r'move.*\w+'
        ],
        'test': [
            r'test',
            r'spec',
            r'assert',
            r'mock',
            r'fixture',
            r'coverage'
        ],
        'chore': [
            r'dependenc',
            r'package\.json',
            r'requirements\.txt',
            r'cargo\.toml',
            r'pipfile',
            r'yarn\.lock'
        ],
        'perf': [
            r'performance',
            r'optimize',
            r'cache',
            r'parallel',
            r'async.*await',
            r'lazy.*load'
        ],
        'ci': [
            r'\.github',
            r'gitlab-ci',
            r'travis',
            r'jenkins',
            r'workflow',
            r'action'
        ],
        'build': [
            r'build',
            r'compile',
            r'webpack',
            r'babel',
            r'typescript',
            r'rustc'
        ]
    }

    # Convert to lowercase for pattern matching
    diff_lower = diff_output.lower()

    for category, category_patterns in patterns.items():
        for pattern in category_patterns:
            matches = len(re.findall(pattern, diff_lower))
            categories[category] += matches

    return categories

def extract_scope(files: List[str]) -> str:
    """Extract common scope from file paths"""
    if not files:
        return ""

    # Extract common directory components
    paths = [Path(f).parts for f in files if f]
    if not paths:
        return ""

    # Find common prefix
    common_parts = []
    for i in range(min(len(p) for p in paths)):
        if all(p[i] == paths[0][i] for p in paths):
            common_parts.append(paths[0][i])
        else:
            break

    # Return first common part as scope
    if common_parts:
        return common_parts[0]

    # Check for common file types
    extensions = [Path(f).suffix for f in files if f]
    if len(set(extensions)) == 1 and extensions[0]:
        return extensions[0][1:]  # Remove dot

    return ""

def detect_breaking_changes(diff_output: str) -> List[str]:
    """Detect potential breaking changes"""
    breaking_patterns = [
        r'remove.*\w+',
        r'delete.*\w+',
        r'break.*change',
        r'breaking',
        r'deprecated',
        r'api.*change',
        r'interface.*change',
        r'backward.*incompatible'
    ]

    breaking_changes = []
    diff_lower = diff_output.lower()

    for pattern in breaking_patterns:
        matches = re.findall(pattern, diff_lower)
        breaking_changes.extend(matches)

    return list(set(breaking_changes))

def suggest_commit_message(files: List[str], staged: bool = True) -> str:
    """Generate a conventional commit message"""
    # Get diff output
    cmd = ['diff']
    if staged:
        cmd.insert(1, '--cached')
    cmd.extend(files)

    diff_output = run_git_command(cmd)
    if not diff_output:
        return "No changes detected"

    # Analyze changes
    categories = analyze_file_changes(diff_output)
    scope = extract_scope(files)
    breaking_changes = detect_breaking_changes(diff_output)

    # Determine primary category
    primary_category = max(categories, key=categories.get)
    if categories[primary_category] == 0:
        primary_category = 'chore'  # Default

    # Generate description
    description = generate_description(primary_category, files, diff_output)

    # Build commit message
    commit_msg = f"{primary_category}"
    if scope:
        commit_msg += f"({scope})"
    commit_msg += f": {description}"

    # Add breaking change note
    if breaking_changes:
        commit_msg += f"\n\nBREAKING CHANGE: {', '.join(breaking_changes[:2])}"

    # Add detailed body if significant changes
    diff_lines = len(diff_output.split('\n'))
    if diff_lines > 50:
        commit_msg += f"\n\nMultiple files changed:\n"
        for file in files[:5]:
            commit_msg += f"- {file}\n"
        if len(files) > 5:
            commit_msg += f"- and {len(files) - 5} more files\n"

    return commit_msg

def generate_description(category: str, files: List[str], diff_output: str) -> str:
    """Generate description based on category and changes"""
    descriptions = {
        'feat': [
            "add new functionality",
            "implement feature",
            "add capability",
            "introduce feature"
        ],
        'fix': [
            "fix bug",
            "resolve issue",
            "correct error",
            "patch bug"
        ],
        'docs': [
            "update documentation",
            "improve docs",
            "add documentation",
            "clarify documentation"
        ],
        'style': [
            "improve formatting",
            "fix code style",
            "update formatting",
            "apply linting fixes"
        ],
        'refactor': [
            "refactor code",
            "improve structure",
            "reorganize code",
            "optimize structure"
        ],
        'test': [
            "add tests",
            "improve test coverage",
            "update tests",
            "fix tests"
        ],
        'chore': [
            "update dependencies",
            "perform maintenance",
            "update configuration",
            "chore task"
        ],
        'perf': [
            "improve performance",
            "optimize code",
            "speed up operations",
            "reduce overhead"
        ],
        'ci': [
            "update CI configuration",
            "improve build process",
            "update workflow",
            "fix pipeline"
        ],
        'build': [
            "update build configuration",
            "fix build issues",
            "improve build process",
            "update compilation"
        ]
    }

    base_desc = descriptions.get(category, ["update code"])[0]

    # Add specificity based on files
    if files:
        file_types = set(Path(f).suffix for f in files)
        if '.py' in file_types:
            return f"{base_desc} in Python code"
        elif '.js' in file_types or '.ts' in file_types:
            return f"{base_desc} in JavaScript/TypeScript"
        elif '.rs' in file_types:
            return f"{base_desc} in Rust code"
        elif '.md' in file_types:
            return f"{base_desc} in documentation"

    return base_desc

def main():
    """Main function"""
    import argparse

    parser = argparse.ArgumentParser(description="Analyze changes and suggest commit messages")
    parser.add_argument("--staged", action="store_true", help="Analyze staged changes")
    parser.add_argument("--unstaged", action="store_true", help="Analyze unstaged changes")
    parser.add_argument("--files", nargs="*", help="Specific files to analyze")

    args = parser.parse_args()

    # Determine which files to analyze
    if args.files:
        files = args.files
        staged = args.staged
    elif args.staged:
        files = get_staged_files()
        staged = True
    elif args.unstaged:
        files = get_unstaged_files()
        staged = False
    else:
        # Default to staged files
        files = get_staged_files()
        staged = True

    if not files:
        print("No files to analyze")
        sys.exit(1)

    # Generate and print commit message
    commit_msg = suggest_commit_message(files, staged)
    print(commit_msg)

if __name__ == "__main__":
    main()