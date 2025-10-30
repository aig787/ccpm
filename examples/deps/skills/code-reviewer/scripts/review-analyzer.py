#!/usr/bin/env python3
"""
Code Review Analyzer
Automated analysis tool for code review
"""

import subprocess
import re
import sys
import os
import json
from pathlib import Path
from typing import List, Dict, Set, Tuple, Optional
import argparse

class CodeReviewAnalyzer:
    def __init__(self, repo_path: str = "."):
        self.repo_path = Path(repo_path).resolve()
        self.issues = []
        self.suggestions = []
        self.nitpicks = []
        self.stats = {
            "files_changed": 0,
            "lines_added": 0,
            "lines_removed": 0,
            "complexity_increase": 0
        }

    def run_git_command(self, cmd: List[str]) -> str:
        """Run git command and return output"""
        try:
            result = subprocess.run(
                ['git'] + cmd,
                cwd=self.repo_path,
                capture_output=True,
                text=True,
                check=True
            )
            return result.stdout.strip()
        except subprocess.CalledProcessError as e:
            print(f"Error running git command: {e}", file=sys.stderr)
            return ""

    def get_changed_files(self, commit_range: str = "HEAD~1..HEAD") -> List[str]:
        """Get list of changed files in commit range"""
        output = self.run_git_command(['diff', '--name-only', commit_range])
        return output.split('\n') if output else []

    def get_file_diff(self, file_path: str, commit_range: str = "HEAD~1..HEAD") -> str:
        """Get diff for specific file"""
        return self.run_git_command(['diff', commit_range, '--', file_path])

    def analyze_diff_stats(self, diff_output: str) -> Dict[str, int]:
        """Analyze diff to count added/removed lines"""
        stats = {"added": 0, "removed": 0}

        for line in diff_output.split('\n'):
            if line.startswith('+') and not line.startswith('+++'):
                stats["added"] += 1
            elif line.startswith('-') and not line.startswith('---'):
                stats["removed"] += 1

        return stats

    def check_complexity(self, file_path: str, diff_output: str) -> None:
        """Check for complexity increases"""
        # Look for nested loops, deep conditionals
        complexity_patterns = [
            r'for.*for',  # Nested loops
            r'if.*if.*if',  # Deep nesting
            r'while.*while',
            r'def.*def.*def',  # Nested functions
        ]

        added_lines = [line for line in diff_output.split('\n')
                      if line.startswith('+') and not line.startswith('+++')]

        for line in added_lines:
            for pattern in complexity_patterns:
                if re.search(pattern, line, re.IGNORECASE):
                    self.issues.append({
                        "type": "complexity",
                        "file": file_path,
                        "line": line,
                        "message": "High complexity detected - consider refactoring"
                    })

    def check_security_issues(self, file_path: str, diff_output: str) -> None:
        """Check for security issues"""
        security_patterns = {
            r'password\s*=\s*["\'][^"\']+["\']': "Hardcoded password detected",
            r'api[_-]?key\s*=\s*["\'][^"\']+["\']': "Hardcoded API key detected",
            r'secret\s*=\s*["\'][^"\']+["\']': "Hardcoded secret detected",
            r'token\s*=\s*["\'][^"\']+["\']': "Hardcoded token detected",
            r'eval\s*\(': "Use of eval() detected - security risk",
            r'exec\s*\(': "Use of exec() detected - security risk",
            r'shell=True': "Shell injection potential - verify input sanitization",
            r'\.innerHTML\s*=': "innerHTML usage - XSS potential",
            r'(?<!_)__import__\s*\(': "Dynamic import detected - verify security",
        }

        added_lines = [line for line in diff_output.split('\n')
                      if line.startswith('+') and not line.startswith('+++')]

        for line in added_lines:
            for pattern, message in security_patterns.items():
                if re.search(pattern, line, re.IGNORECASE):
                    self.issues.append({
                        "type": "security",
                        "file": file_path,
                        "line": line,
                        "message": message
                    })

    def check_performance_issues(self, file_path: str, diff_output: str) -> None:
        """Check for performance issues"""
        performance_patterns = {
            r'for.*in.*range\(.*\)\s*:\s*.*\.append\(': "List append in loop - consider list comprehension",
            r'\.select.*\[': "SELECT * detected - specify columns needed",
            r'(?<!\w)time\.sleep\(': "Time sleep in production code?",
            r'(?<!\w)os\.system\(': "Use subprocess instead of os.system",
            r'\+.*\+.*\+': "Multiple string concatenation - use f-string or join",
            r'request\.get.*verify=False': "SSL verification disabled - security risk",
        }

        added_lines = [line for line in diff_output.split('\n')
                      if line.startswith('+') and not line.startswith('+++')]

        for line in added_lines:
            for pattern, message in performance_patterns.items():
                if re.search(pattern, line, re.IGNORECASE):
                    self.suggestions.append({
                        "type": "performance",
                        "file": file_path,
                        "line": line,
                        "message": message
                    })

    def check_code_style(self, file_path: str, diff_output: str) -> None:
        """Check for code style issues"""
        style_patterns = {
            r'^\+\s*print\s*\(': "Print statement in production code",
            r'^\+\s*[a-zA-Z_]\w*\s*=\s*None\s*#.*todo': "TODO comment found",
            r'^\+\s*except:': "Bare except clause - specify exception type",
            r'^\+\s*if\s+.*==\s*None': "Use 'is None' instead of '== None'",
            r'^\+\s*if\s+.*!=\s*None': "Use 'is not None' instead of '!= None'",
            r'^\+\s*return\s*$': "Empty return - be explicit about return value",
        }

        added_lines = [line for line in diff_output.split('\n')
                      if line.startswith('+') and not line.startswith('+++')]

        for line in added_lines:
            for pattern, message in style_patterns.items():
                if re.search(pattern, line):
                    self.nitpicks.append({
                        "type": "style",
                        "file": file_path,
                        "line": line,
                        "message": message
                    })

    def check_testing(self, file_path: str, diff_output: str) -> None:
        """Check if tests are added for new functionality"""
        if 'test' not in file_path.lower():
            # Check if this adds new functions/classes without tests
            added_lines = [line for line in diff_output.split('\n')
                          if line.startswith('+') and not line.startswith('+++')]

            new_functions = 0
            new_classes = 0

            for line in added_lines:
                if re.search(r'def\s+\w+\s*\(', line):
                    new_functions += 1
                elif re.search(r'class\s+\w+\s*\(', line):
                    new_classes += 1

            if new_functions > 0 or new_classes > 0:
                self.suggestions.append({
                    "type": "testing",
                    "file": file_path,
                    "line": "",
                    "message": f"New functionality detected ({new_functions} functions, {new_classes} classes) - consider adding tests"
                })

    def check_documentation(self, file_path: str, diff_output: str) -> None:
        """Check if documentation is adequate"""
        if file_path.endswith('.py'):
            added_lines = [line for line in diff_output.split('\n')
                          if line.startswith('+') and not line.startswith('+++')]

            new_functions = []
            in_function = False
            function_docstring = False

            for line in added_lines:
                if re.search(r'def\s+\w+\s*\(', line):
                    new_functions.append(line.strip('+'))
                    in_function = True
                    function_docstring = False
                elif in_function and '"""' in line:
                    function_docstring = True
                elif in_function and (line.startswith('+    ') and not line.strip('+')):
                    # End of function indentation
                    if not function_docstring and len(new_functions) > 0:
                        self.nitpicks.append({
                            "type": "documentation",
                            "file": file_path,
                            "line": new_functions[-1],
                            "message": "Function missing docstring"
                        })
                    in_function = False
                    function_docstring = False

    def analyze_file(self, file_path: str, commit_range: str = "HEAD~1..HEAD") -> None:
        """Analyze a single file"""
        if not Path(self.repo_path / file_path).exists():
            return

        diff_output = self.get_file_diff(file_path, commit_range)
        if not diff_output:
            return

        stats = self.analyze_diff_stats(diff_output)
        self.stats["lines_added"] += stats["added"]
        self.stats["lines_removed"] += stats["removed"]

        # Run various checks
        self.check_complexity(file_path, diff_output)
        self.check_security_issues(file_path, diff_output)
        self.check_performance_issues(file_path, diff_output)
        self.check_code_style(file_path, diff_output)
        self.check_testing(file_path, diff_output)
        self.check_documentation(file_path, diff_output)

    def analyze_changes(self, commit_range: str = "HEAD~1..HEAD") -> Dict:
        """Analyze all changes in commit range"""
        files = self.get_changed_files(commit_range)
        self.stats["files_changed"] = len(files)

        for file_path in files:
            self.analyze_file(file_path, commit_range)

        return {
            "stats": self.stats,
            "issues": self.issues,
            "suggestions": self.suggestions,
            "nitpicks": self.nitpicks
        }

    def generate_review_report(self, analysis: Dict) -> str:
        """Generate formatted review report"""
        report = []
        report.append("# Code Review Report\n")

        # Summary
        stats = analysis["stats"]
        report.append(f"## Summary")
        report.append(f"- Files changed: {stats['files_changed']}")
        report.append(f"- Lines added: {stats['lines_added']}")
        report.append(f"- Lines removed: {stats['lines_removed']}")
        report.append(f"- Issues found: {len(analysis['issues'])}")
        report.append(f"- Suggestions: {len(analysis['suggestions'])}")
        report.append(f"- Nitpicks: {len(analysis['nitpicks'])}\n")

        # High priority issues
        if analysis["issues"]:
            report.append("## 🚨 High Priority Issues")
            for issue in analysis["issues"]:
                report.append(f"**{issue['file']}**")
                report.append(f"- {issue['message']}")
                if issue['line']:
                    report.append(f"  ```")
                    report.append(f"  {issue['line']}")
                    report.append(f"  ```")
                report.append("")

        # Suggestions
        if analysis["suggestions"]:
            report.append("## 💡 Suggestions")
            for suggestion in analysis["suggestions"]:
                report.append(f"**{suggestion['file']}**")
                report.append(f"- {suggestion['message']}")
                if suggestion['line']:
                    report.append(f"  ```")
                    report.append(f"  {suggestion['line']}")
                    report.append(f"  ```")
                report.append("")

        # Nitpicks
        if analysis["nitpicks"]:
            report.append("## ✨ Nitpicks")
            for nitpick in analysis["nitpicks"]:
                report.append(f"**{nitpick['file']}**")
                report.append(f"- {nitpick['message']}")
                report.append("")

        # Overall assessment
        report.append("## Overall Assessment")
        if analysis["issues"]:
            report.append("⚠️ **Needs changes before merge** - Address high priority issues")
        elif analysis["suggestions"]:
            report.append("✅ **Good to merge** - Consider suggestions for improvement")
        else:
            report.append("🎉 **Excellent** - Ready to merge")

        return "\n".join(report)

def main():
    parser = argparse.ArgumentParser(description="Analyze code changes for review")
    parser.add_argument("--repo", default=".", help="Repository path")
    parser.add_argument("--range", default="HEAD~1..HEAD", help="Commit range to analyze")
    parser.add_argument("--format", choices=["text", "json"], default="text", help="Output format")
    parser.add_argument("--output", help="Output file path")

    args = parser.parse_args()

    analyzer = CodeReviewAnalyzer(args.repo)
    analysis = analyzer.analyze_changes(args.range)

    if args.format == "json":
        output = json.dumps(analysis, indent=2)
    else:
        output = analyzer.generate_review_report(analysis)

    if args.output:
        with open(args.output, 'w') as f:
            f.write(output)
        print(f"Review report saved to {args.output}")
    else:
        print(output)

if __name__ == "__main__":
    main()