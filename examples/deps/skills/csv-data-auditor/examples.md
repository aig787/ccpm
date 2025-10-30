# CSV Data Auditor Examples

## Example 1: Basic CSV Validation

### Command:
```bash
python scripts/csv_validator.py data/customers.csv
```

### Sample Output:
```json
{
  "file_info": {
    "path": "data/customers.csv",
    "encoding": "utf-8",
    "delimiter": ",",
    "rows": 5000,
    "columns": 12,
    "file_size_mb": 2.34
  },
  "summary": {
    "critical_issues": 0,
    "errors": 2,
    "warnings": 8,
    "total_issues": 10
  },
  "issues": [
    {
      "type": "duplicate_ids",
      "column": "id",
      "count": 15,
      "severity": "error"
    },
    {
      "type": "empty_file",
      "message": "CSV file is empty",
      "severity": "critical"
    }
  ],
  "warnings": [
    {
      "type": "missing_values",
      "column": "email",
      "count": 150,
      "percentage": 3.0,
      "severity": "warning"
    },
    {
      "type": "outliers",
      "column": "age",
      "count": 5,
      "percentage": 0.1,
      "values": [0, 150, -5, 200, 999],
      "severity": "info"
    }
  ],
  "recommendations": [
    "Remove or investigate duplicate IDs to ensure data integrity",
    "Investigate outliers - they may indicate data entry errors or legitimate special cases"
  ]
}
```

## Example 2: Validation with Custom Rules

### Business Rules File (`rules.json`):
```json
{
  "business_rules": {
    "age_range": {
      "column": "age",
      "type": "range",
      "min": 18,
      "max": 100,
      "description": "Customer age must be between 18 and 100"
    },
    "email_format": {
      "column": "email",
      "type": "pattern",
      "pattern": "^[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\\.[a-zA-Z]{2,}$",
      "description": "Valid email format required"
    },
    "status_values": {
      "column": "status",
      "type": "allowed_values",
      "values": ["active", "inactive", "pending"],
      "description": "Status must be valid"
    }
  }
}
```

### Command:
```bash
python scripts/csv_validator.py data/users.csv --rules rules.json --format markdown --output validation_report.md
```

### Generated Report (`validation_report.md`):
```markdown
# CSV Data Audit Report

## File Information
- File: `data/users.csv`
- Size: 1.23 MB
- Rows: 2,500
- Columns: 8
- Encoding: utf-8

## Summary
- Critical Issues: 0
- Errors: 2
- Warnings: 5
- Total Issues: 7

## ❌ Errors
**Business Rule Violation**
- Column: `age`
- Values outside range 18-100

**Business Rule Violation**
- Column: `status`
- Invalid values found. Allowed: ['active', 'inactive', 'pending']

## ⚠️ Warnings
**Missing Values**
- Column: `email`
- Affected: 2.5%
- 62 rows have missing email addresses

**Potential Date Column**
- Column: `join_date`
- Column appears to contain dates but is stored as text

**Outliers**
- Column: `salary`
- Count: 12
- Affected: 0.48%
- Values: [1000000, 2500000, 5000000]

## 💡 Recommendations
- Remove or investigate duplicate IDs to ensure data integrity
- Standardize data types for consistent analysis
- Consider data imputation or collection strategies for columns with high missing values: ['email']

## Overall Assessment
🟡 **Fair** - Errors need to be fixed before use
```

## Example 3: Batch Processing Multiple Files

### Python Script (`batch_validate.py`):
```python
#!/usr/bin/env python3
import os
import json
from pathlib import Path
from csv_validator import CSVValidator

def validate_directory(directory_path, output_dir):
    """Validate all CSV files in a directory"""
    results = {}

    for csv_file in Path(directory_path).glob("*.csv"):
        print(f"Validating {csv_file.name}...")

        validator = CSVValidator(csv_file)

        if validator.load_csv():
            validator.validate_structure()
            validator.check_missing_values()
            validator.check_data_types()
            validator.check_duplicates()
            validator.check_outliers()
            validator.check_consistency()

            report = validator.generate_report()
            results[csv_file.name] = report

            # Save individual report
            output_file = Path(output_dir) / f"{csv_file.stem}_report.json"
            with open(output_file, 'w') as f:
                json.dump(report, f, indent=2, default=str)

    # Save summary report
    summary_file = Path(output_dir) / "validation_summary.json"
    with open(summary_file, 'w') as f:
        json.dump(results, f, indent=2, default=str)

    return results

# Usage
if __name__ == "__main__":
    results = validate_directory("data/", "reports/")
    print(f"Validated {len(results)} files")
```

## Example 4: Different Delimiters and Encodings

### Command Options:
```bash
# Semicolon delimiter
python scripts/csv_validator.py data/european_data.csv --delimiter ";"

# Tab-separated
python scripts/csv_validator.py data/tabs.txt --delimiter $'\t'

# Different encoding
python scripts/csv_validator.py data/latin1_data.csv --encoding latin-1

# Combined options
python scripts/csv_validator.py data/complex.csv --delimiter "|" --encoding cp1252 --output report.md
```

## Example 5: Custom Validation Script

### Integration with Pandas Workflow:
```python
#!/usr/bin/env python3
import pandas as pd
from csv_validator import CSVValidator

def validate_and_clean_data(csv_path):
    """Validate and clean a CSV file"""

    # First, validate the data
    validator = CSVValidator(csv_path)

    if not validator.load_csv():
        raise Exception("Cannot load CSV file")

    validator.validate_structure()
    validator.check_missing_values()
    validator.check_duplicates()

    report = validator.generate_report()

    # Print summary
    print(f"Validation complete:")
    print(f"- Rows: {report['file_info']['rows']}")
    print(f"- Issues found: {report['summary']['total_issues']}")

    # If critical issues, stop processing
    if report['summary']['critical_issues'] > 0:
        print("❌ Critical issues found - cannot proceed")
        return None

    # Clean the data based on validation results
    df = validator.df.copy()

    # Remove exact duplicates
    df = df.drop_duplicates()

    # Handle missing values
    for warning in report['warnings']:
        if warning['type'] == 'missing_values':
            col = warning['column']
            if warning['percentage'] < 5:
                # Fill with mode for categorical, median for numeric
                if df[col].dtype == 'object':
                    df[col].fillna(df[col].mode()[0], inplace=True)
                else:
                    df[col].fillna(df[col].median(), inplace=True)

    return df, report

# Usage
cleaned_df, validation_report = validate_and_clean_data("data/sales.csv")
if cleaned_df is not None:
    print("✅ Data validated and cleaned successfully")
    # Continue with analysis...
```

## Example 6: Monitoring Data Quality Over Time

### Dashboard Data:
```python
import json
import matplotlib.pyplot as plt
from datetime import datetime

def track_quality_metrics(report_files):
    """Track data quality metrics over time"""

    metrics = {
        'dates': [],
        'missing_values': [],
        'duplicates': [],
        'total_issues': []
    }

    for report_file in sorted(report_files):
        with open(report_file, 'r') as f:
            report = json.load(f)

        file_date = datetime.fromisoformat(report['file_info'].get('validated_at', '2024-01-01'))

        metrics['dates'].append(file_date)
        metrics['missing_values'].append(report['summary']['warnings'])
        metrics['duplicates'].append(len([i for i in report['issues'] if i['type'] == 'duplicate_ids']))
        metrics['total_issues'].append(report['summary']['total_issues'])

    # Plot trends
    plt.figure(figsize=(12, 6))
    plt.plot(metrics['dates'], metrics['total_issues'], marker='o', label='Total Issues')
    plt.plot(metrics['dates'], metrics['missing_values'], marker='s', label='Warnings')
    plt.plot(metrics['dates'], metrics['duplicates'], marker='^', label='Duplicates')

    plt.title('Data Quality Trends Over Time')
    plt.xlabel('Date')
    plt.ylabel('Count')
    plt.legend()
    plt.grid(True, alpha=0.3)
    plt.xticks(rotation=45)
    plt.tight_layout()
    plt.show()

# Usage
report_files = ['reports/jan_report.json', 'reports/feb_report.json', 'reports/mar_report.json']
track_quality_metrics(report_files)
```

## Common Validation Scenarios

### 1. Customer Data Validation
- Check for valid email formats
- Ensure unique customer IDs
- Validate phone numbers
- Check age ranges

### 2. Financial Data Validation
- Validate currency amounts are positive
- Check transaction dates aren't in future
- Ensure account numbers follow patterns
- Detect duplicate transactions

### 3. Product Data Validation
- Validate SKUs follow format
- Check prices are reasonable
- Ensure category names are consistent
- Verify stock quantities are non-negative

### 4. Time Series Data Validation
- Check for chronological order
- Validate date formats
- Detect gaps in time series
- Check for duplicate timestamps

## Integration Tips

### CI/CD Pipeline Integration
```yaml
# .github/workflows/data-validation.yml
name: Data Validation
on: [push, pull_request]

jobs:
  validate-data:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v2
      - name: Setup Python
        uses: actions/setup-python@v2
        with:
          python-version: '3.9'
      - name: Install dependencies
        run: pip install pandas numpy
      - name: Validate CSV files
        run: |
          python scripts/csv_validator.py data/*.csv --format markdown --output validation_report.md
      - name: Upload validation report
        uses: actions/upload-artifact@v2
        with:
          name: validation-report
          path: validation_report.md
```

### Database Integration
```python
def validate_database_export(csv_export_path):
    """Validate data exported from database"""
    validator = CSVValidator(csv_export_path)

    # Custom validation for database constraints
    db_rules = {
        "business_rules": {
            "foreign_key": {
                "column": "user_id",
                "type": "reference",
                "reference_table": "users",
                "description": "User ID must exist in users table"
            },
            "not_null_constraints": {
                "columns": ["id", "created_at", "updated_at"],
                "description": "Required fields cannot be null"
            }
        }
    }

    validator.check_business_rules(db_rules)
    return validator.generate_report()
```