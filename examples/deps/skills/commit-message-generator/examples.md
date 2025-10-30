# Commit Message Generator Examples

## Example 1: Simple Feature Addition

### Changes Made
- Added new authentication endpoint `/api/login`
- Created JWT token validation middleware
- Updated user model with password field

### Generated Message
```
feat(auth): add JWT-based authentication system

Implements secure authentication with JWT tokens including:
- Login endpoint with email/password validation
- Token generation and validation middleware
- Password hashing with bcrypt
- User session management

Closes #123
```

## Example 2: Bug Fix

### Changes Made
- Fixed memory leak in data processing module
- Updated cleanup logic for file handles
- Added proper error handling

### Generated Message
```
fix(data): resolve memory leak in file processing

Fixed memory leak caused by unclosed file handles in the data
processing pipeline. Added proper context manager usage and
error handling to ensure resources are cleaned up correctly.

Fixes #456
```

## Example 3: Breaking Change

### Changes Made
- Changed API response format from array to object
- Updated all endpoints to return consistent structure
- Modified client-side code to handle new format

### Generated Message
```
feat(api): standardize response format to object structure

All API endpoints now return standardized object format:
```json
{
  "success": boolean,
  "data": any,
  "error": string | null
}
```

BREAKING CHANGE: This changes the response format from direct
data return to wrapped object structure. Client code needs to be
updated to access `response.data` instead of using response directly.

Migration guide available in docs/api-migration.md
```

## Example 4: Refactoring

### Changes Made
- Extracted common validation logic into utility functions
- Renamed unclear variable names
- Split large function into smaller functions

### Generated Message
```
refactor(utils): extract common validation logic

Created reusable validation utilities to reduce code duplication
across the codebase. Improved code readability with better
function and variable names.
```

## Example 5: Documentation

### Changes Made
- Updated README with installation instructions
- Added API documentation
- Fixed typos in existing docs

### Generated Message
```
docs: update installation guide and API documentation

- Added step-by-step installation instructions for all platforms
- Documented all public API endpoints with examples
- Fixed typos and improved clarity in existing documentation
```

## Example 6: Multiple Files with Same Scope

### Changes Made
- Updated 5 files in the payment module
- Added new payment methods
- Fixed calculation errors

### Generated Message
```
feat(payment): add support for Apple Pay and Google Pay

Expanded payment processing to support mobile payment methods.
Updated validation logic and error handling for new payment types.

Files changed:
- payment/processors.py
- payment/validators.py
- payment/models.py
- tests/test_payment.py
- docs/payment.md
```

## Usage Examples

### Using the Python Script

```bash
# Analyze staged changes
python scripts/commit-analyzer.py --staged

# Analyze specific files
python scripts/commit-analyzer.py --files src/main.py src/utils.py

# Analyze unstaged changes
python scripts/commit-analyzer.py --unstaged
```

### Manual Template Usage

1. Copy from `templates/commit-template.md`
2. Fill in your changes
3. Use the format guidelines to ensure consistency