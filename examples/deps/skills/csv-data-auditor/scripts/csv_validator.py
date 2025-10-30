#!/usr/bin/env python3
"""
CSV Data Validator
Comprehensive CSV validation and quality assessment tool
"""

import pandas as pd
import numpy as np
import re
import json
import sys
import argparse
from pathlib import Path
from typing import Dict, List, Tuple, Optional, Any
from datetime import datetime
import warnings
warnings.filterwarnings('ignore')

class CSVValidator:
    def __init__(self, file_path: str, delimiter: str = ',', encoding: str = 'utf-8'):
        self.file_path = Path(file_path)
        self.delimiter = delimiter
        self.encoding = encoding
        self.df = None
        self.issues = []
        self.warnings = []
        self.stats = {}

    def load_csv(self) -> bool:
        """Load CSV file with error handling"""
        try:
            # Try different encodings if utf-8 fails
            encodings = [self.encoding, 'latin-1', 'cp1252', 'iso-8859-1']

            for encoding in encodings:
                try:
                    self.df = pd.read_csv(self.file_path, delimiter=self.delimiter, encoding=encoding)
                    self.encoding = encoding
                    break
                except UnicodeDecodeError:
                    continue
            else:
                raise Exception("Could not decode file with any supported encoding")

            return True
        except Exception as e:
            self.issues.append({
                "type": "file_error",
                "message": f"Failed to load CSV: {str(e)}",
                "severity": "critical"
            })
            return False

    def validate_structure(self) -> None:
        """Validate CSV structure"""
        if self.df is None:
            return

        # Basic structure
        self.stats.update({
            "rows": len(self.df),
            "columns": len(self.df.columns),
            "headers": list(self.df.columns),
            "file_size_mb": self.file_path.stat().st_size / (1024 * 1024)
        })

        # Check for empty DataFrame
        if len(self.df) == 0:
            self.issues.append({
                "type": "empty_file",
                "message": "CSV file is empty",
                "severity": "critical"
            })
            return

        # Check for empty columns
        empty_cols = self.df.columns[self.df.isnull().all()].tolist()
        if empty_cols:
            self.warnings.append({
                "type": "empty_columns",
                "message": f"Empty columns found: {empty_cols}",
                "severity": "warning"
            })

        # Check for duplicate headers
        duplicate_headers = [col for col in self.df.columns if list(self.df.columns).count(col) > 1]
        if duplicate_headers:
            self.issues.append({
                "type": "duplicate_headers",
                "message": f"Duplicate headers: {set(duplicate_headers)}",
                "severity": "error"
            })

    def check_missing_values(self) -> None:
        """Check for missing values"""
        if self.df is None:
            return

        missing_counts = self.df.isnull().sum()
        total_rows = len(self.df)

        for col, count in missing_counts.items():
            if count > 0:
                percentage = (count / total_rows) * 100

                if percentage > 50:
                    severity = "critical"
                elif percentage > 20:
                    severity = "error"
                elif percentage > 5:
                    severity = "warning"
                else:
                    severity = "info"

                self.warnings.append({
                    "type": "missing_values",
                    "column": col,
                    "count": int(count),
                    "percentage": round(percentage, 2),
                    "severity": severity
                })

    def check_data_types(self) -> None:
        """Validate data types and consistency"""
        if self.df is None:
            return

        for col in self.df.columns:
            series = self.df[col].dropna()

            if len(series) == 0:
                continue

            # Check for mixed types
            if series.dtype == 'object':
                # Check if it's supposed to be numeric
                numeric_converted = pd.to_numeric(series, errors='coerce')
                if not numeric_converted.isna().all():
                    non_numeric = series[~numeric_converted.notna()]
                    if len(non_numeric) > 0:
                        self.warnings.append({
                            "type": "mixed_types_numeric",
                            "column": col,
                            "message": f"Column contains non-numeric values: {non_numeric.unique()[:5].tolist()}",
                            "severity": "warning"
                        })

                # Check for dates
                date_patterns = [
                    r'\d{4}-\d{2}-\d{2}',  # ISO
                    r'\d{2}/\d{2}/\d{4}',  # US
                    r'\d{2}-\d{2}-\d{4}',  # EU
                ]

                sample_values = series.head(10).astype(str)
                date_matches = 0

                for pattern in date_patterns:
                    date_matches += sample_values.str.match(pattern).sum()

                if date_matches > len(sample_values) * 0.5:
                    self.warnings.append({
                        "type": "potential_date_column",
                        "column": col,
                        "message": "Column appears to contain dates but is stored as text",
                        "severity": "info"
                    })

    def check_duplicates(self) -> None:
        """Check for duplicate records"""
        if self.df is None:
            return

        # Check for exact duplicates
        duplicate_rows = self.df.duplicated().sum()
        if duplicate_rows > 0:
            self.warnings.append({
                "type": "duplicate_rows",
                "count": int(duplicate_rows),
                "percentage": round((duplicate_rows / len(self.df)) * 100, 2),
                "severity": "warning" if duplicate_rows < len(self.df) * 0.1 else "error"
            })

        # Check for potential ID columns and duplicates
        for col in self.df.columns:
            if any(keyword in col.lower() for keyword in ['id', 'key', 'code', 'identifier']):
                duplicate_ids = self.df[col].duplicated().sum()
                if duplicate_ids > 0:
                    self.issues.append({
                        "type": "duplicate_ids",
                        "column": col,
                        "count": int(duplicate_ids),
                        "severity": "error"
                    })

    def check_outliers(self) -> None:
        """Check for outliers in numeric columns"""
        if self.df is None:
            return

        numeric_cols = self.df.select_dtypes(include=[np.number]).columns

        for col in numeric_cols:
            series = self.df[col].dropna()
            if len(series) < 4:  # Need enough data for outlier detection
                continue

            # IQR method
            Q1 = series.quantile(0.25)
            Q3 = series.quantile(0.75)
            IQR = Q3 - Q1
            lower_bound = Q1 - 1.5 * IQR
            upper_bound = Q3 + 1.5 * IQR

            outliers = series[(series < lower_bound) | (series > upper_bound)]

            if len(outliers) > 0:
                self.warnings.append({
                    "type": "outliers",
                    "column": col,
                    "count": len(outliers),
                    "percentage": round((len(outliers) / len(series)) * 100, 2),
                    "values": outliers.tolist()[:10],  # Show first 10
                    "severity": "info"
                })

    def check_consistency(self) -> None:
        """Check for data consistency issues"""
        if self.df is None:
            return

        # Check for inconsistent string formatting
        string_cols = self.df.select_dtypes(include=['object']).columns

        for col in string_cols:
            series = self.df[col].dropna()
            if len(series) == 0:
                continue

            # Check for leading/trailing whitespace
            whitespace_issues = series.str.strip().ne(series).sum()
            if whitespace_issues > 0:
                self.warnings.append({
                    "type": "whitespace_issues",
                    "column": col,
                    "count": int(whitespace_issues),
                    "message": f"Rows have leading/trailing whitespace",
                    "severity": "info"
                })

            # Check for inconsistent capitalization
            if series.str.isupper().any() and series.str.islower().any():
                sample_values = series.head(20).unique()
                self.warnings.append({
                    "type": "inconsistent_case",
                    "column": col,
                    "message": "Mixed case usage detected",
                    "examples": sample_values[:5].tolist(),
                    "severity": "info"
                })

    def check_business_rules(self, rules: Optional[Dict] = None) -> None:
        """Check custom business rules"""
        if self.df is None or not rules:
            return

        for rule_name, rule_config in rules.items():
            col = rule_config.get('column')
            rule_type = rule_config.get('type')

            if col not in self.df.columns:
                continue

            series = self.df[col].dropna()

            if rule_type == 'range':
                min_val = rule_config.get('min')
                max_val = rule_config.get('max')
                violations = series[(series < min_val) | (series > max_val)]

                if len(violations) > 0:
                    self.issues.append({
                        "type": "business_rule_violation",
                        "rule": rule_name,
                        "column": col,
                        "count": len(violations),
                        "message": f"Values outside range {min_val}-{max_val}",
                        "severity": "error"
                    })

            elif rule_type == 'pattern':
                pattern = rule_config.get('pattern')
                invalid = ~series.str.match(pattern, na=False)
                violations = series[invalid]

                if len(violations) > 0:
                    self.issues.append({
                        "type": "business_rule_violation",
                        "rule": rule_name,
                        "column": col,
                        "count": len(violations),
                        "message": f"Values don't match required pattern",
                        "severity": "error"
                    })

            elif rule_type == 'allowed_values':
                allowed = rule_config.get('values', [])
                invalid = ~series.isin(allowed)
                violations = series[invalid]

                if len(violations) > 0:
                    self.issues.append({
                        "type": "business_rule_violation",
                        "rule": rule_name,
                        "column": col,
                        "count": len(violations),
                        "message": f"Invalid values found. Allowed: {allowed}",
                        "severity": "error"
                    })

    def generate_report(self) -> Dict:
        """Generate comprehensive validation report"""
        report = {
            "file_info": {
                "path": str(self.file_path),
                "encoding": self.encoding,
                "delimiter": self.delimiter,
                **self.stats
            },
            "summary": {
                "critical_issues": len([i for i in self.issues if i.get('severity') == 'critical']),
                "errors": len([i for i in self.issues if i.get('severity') == 'error']),
                "warnings": len(self.warnings),
                "total_issues": len(self.issues) + len(self.warnings)
            },
            "issues": self.issues,
            "warnings": self.warnings,
            "recommendations": self._generate_recommendations()
        }

        return report

    def _generate_recommendations(self) -> List[str]:
        """Generate data quality recommendations"""
        recommendations = []

        # Based on missing values
        high_missing = [w for w in self.warnings if w.get('type') == 'missing_values' and w.get('percentage', 0) > 20]
        if high_missing:
            recommendations.append(f"Consider data imputation or collection strategies for columns with high missing values: {[w['column'] for w in high_missing]}")

        # Based on duplicates
        duplicate_issues = [i for i in self.issues if i.get('type') == 'duplicate_ids']
        if duplicate_issues:
            recommendations.append("Remove or investigate duplicate IDs to ensure data integrity")

        # Based on data types
        type_issues = [w for w in self.warnings if w.get('type') in ['mixed_types_numeric', 'potential_date_column']]
        if type_issues:
            recommendations.append("Standardize data types for consistent analysis")

        # Based on outliers
        outlier_issues = [w for w in self.warnings if w.get('type') == 'outliers']
        if outlier_issues:
            recommendations.append("Investigate outliers - they may indicate data entry errors or legitimate special cases")

        # General recommendations
        if not recommendations:
            recommendations.append("Data quality appears good - consider setting up automated validation for future uploads")

        return recommendations

    def save_report(self, output_path: str, format: str = 'json') -> None:
        """Save validation report"""
        report = self.generate_report()

        if format == 'json':
            with open(output_path, 'w') as f:
                json.dump(report, f, indent=2, default=str)
        elif format == 'markdown':
            with open(output_path, 'w') as f:
                f.write(self._generate_markdown_report(report))

    def _generate_markdown_report(self, report: Dict) -> str:
        """Generate markdown format report"""
        md = []
        md.append("# CSV Data Audit Report\n")

        # File info
        md.append("## File Information")
        info = report['file_info']
        md.append(f"- File: `{info['path']}`")
        md.append(f"- Size: {info.get('file_size_mb', 0):.2f} MB")
        md.append(f"- Rows: {info.get('rows', 0):,}")
        md.append(f"- Columns: {info.get('columns', 0)}")
        md.append(f"- Encoding: {info.get('encoding', 'unknown')}")
        md.append("")

        # Summary
        md.append("## Summary")
        summary = report['summary']
        md.append(f"- Critical Issues: {summary['critical_issues']}")
        md.append(f"- Errors: {summary['errors']}")
        md.append(f"- Warnings: {summary['warnings']}")
        md.append(f"- Total Issues: {summary['total_issues']}")
        md.append("")

        # Critical Issues
        critical_issues = [i for i in report['issues'] if i.get('severity') == 'critical']
        if critical_issues:
            md.append("## 🚨 Critical Issues")
            for issue in critical_issues:
                md.append(f"**{issue.get('type', 'unknown').replace('_', ' ').title()}**")
                md.append(f"- {issue.get('message', 'No message')}")
                md.append("")

        # Errors
        errors = [i for i in report['issues'] if i.get('severity') == 'error']
        if errors:
            md.append("## ❌ Errors")
            for error in errors:
                md.append(f"**{error.get('type', 'unknown').replace('_', ' ').title()}**")
                md.append(f"- {error.get('message', 'No message')}")
                if 'column' in error:
                    md.append(f"  - Column: `{error['column']}`")
                md.append("")

        # Warnings
        if report['warnings']:
            md.append("## ⚠️ Warnings")
            for warning in report['warnings'][:20]:  # Limit to first 20
                md.append(f"**{warning.get('type', 'unknown').replace('_', ' ').title()}**")
                if 'column' in warning:
                    md.append(f"- Column: `{warning['column']}`")
                md.append(f"- {warning.get('message', 'No message')}")
                if 'percentage' in warning:
                    md.append(f"  - Affected: {warning['percentage']}%")
                md.append("")

            if len(report['warnings']) > 20:
                md.append(f"... and {len(report['warnings']) - 20} more warnings")
                md.append("")

        # Recommendations
        if report['recommendations']:
            md.append("## 💡 Recommendations")
            for rec in report['recommendations']:
                md.append(f"- {rec}")
            md.append("")

        # Overall Assessment
        md.append("## Overall Assessment")
        if summary['critical_issues'] > 0:
            md.append("🔴 **Poor** - Critical issues must be addressed")
        elif summary['errors'] > 0:
            md.append("🟡 **Fair** - Errors need to be fixed before use")
        elif summary['warnings'] > 10:
            md.append("🟠 **Good** - Many warnings suggest data quality issues")
        elif summary['warnings'] > 0:
            md.append("🟢 **Very Good** - Minor issues to consider")
        else:
            md.append("✅ **Excellent** - No significant issues detected")

        return "\n".join(md)

def main():
    parser = argparse.ArgumentParser(description="Validate CSV data quality")
    parser.add_argument("file", help="CSV file to validate")
    parser.add_argument("--delimiter", default=",", help="CSV delimiter")
    parser.add_argument("--encoding", default="utf-8", help="File encoding")
    parser.add_argument("--output", help="Output report file")
    parser.add_argument("--format", choices=["json", "markdown"], default="markdown", help="Report format")
    parser.add_argument("--rules", help="JSON file with business rules")

    args = parser.parse_args()

    # Load business rules if provided
    rules = None
    if args.rules:
        with open(args.rules, 'r') as f:
            rules = json.load(f)

    # Validate CSV
    validator = CSVValidator(args.file, args.delimiter, args.encoding)

    if not validator.load_csv():
        print(f"Failed to load CSV file: {args.file}")
        sys.exit(1)

    # Run validations
    validator.validate_structure()
    validator.check_missing_values()
    validator.check_data_types()
    validator.check_duplicates()
    validator.check_outliers()
    validator.check_consistency()
    validator.check_business_rules(rules)

    # Generate and save report
    if args.output:
        validator.save_report(args.output, args.format)
        print(f"Validation report saved to {args.output}")
    else:
        report = validator.generate_report()
        print(json.dumps(report, indent=2, default=str))

if __name__ == "__main__":
    main()