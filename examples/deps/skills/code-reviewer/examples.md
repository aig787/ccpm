# Code Reviewer Examples

## Example 1: Feature Addition Review

### Pull Request: Add User Authentication System

#### Review Report:
```markdown
# Code Review Report

## Summary
This PR implements a JWT-based authentication system with login, logout, and token refresh functionality. The implementation is well-structured but has some security concerns that need addressing.

## 🚨 High Priority Issues

**auth/jwt_handler.py**
- **Security**: JWT secret is hardcoded. Move to environment variable.
  ```python
  SECRET_KEY = "your-secret-key"  # Should be from environment
  ```

**auth/middleware.py**
- **Error Handling**: Missing error handling for malformed tokens
  ```python
  try:
      payload = decode_token(token)
  except Exception:
      return jsonify({"error": "Invalid token"}), 401
  ```

## 💡 Suggestions

**routes/auth.py**
- Consider adding rate limiting to login endpoint
- Implement refresh token rotation for better security
- Add password complexity requirements

**models/user.py**
- Add email verification field
- Consider adding user roles/permissions

## ✨ Nitpicks

**utils/validation.py**
- Function name `validate_email` could be more descriptive
- Add type hints for better documentation

## Overall Assessment
⚠️ **Needs changes before merge** - Address security issues
```

## Example 2: Bug Fix Review

### Pull Request: Fix Memory Leak in Data Processor

#### Review Report:
```markdown
# Code Review Report

## Summary
This PR fixes a critical memory leak in the data processing module by properly closing file handles and implementing context managers. Good fix with comprehensive error handling.

## 🚨 High Priority Issues
None found

## 💡 Suggestions

**data/processor.py**
- Consider adding a test specifically for memory usage
- Document the fix in the changelog
- Add monitoring for file handle usage in production

## ✨ Nitpicks

**tests/test_processor.py**
- Test name `test_fix` could be more descriptive
- Add a comment explaining the memory leak scenario

## Overall Assessment
✅ **Good to merge** - Well implemented fix
```

## Example 3: Performance Improvement Review

### Pull Request: Optimize Database Queries

#### Review Report:
```markdown
# Code Review Report

## Summary
This PR optimizes database queries by adding indexes and implementing query result caching. Significant performance improvement but missing migration for new indexes.

## 🚨 High Priority Issues

**migrations/**
- Missing migration file for new database indexes
- Indexes need to be created before deployment

## 💡 Suggestions

**models/query.py**
- Consider implementing query result pagination
- Add query performance monitoring
- Document cache invalidation strategy

## ✨ Nitpicks

**config/cache.py**
- Cache TTL of 3600 seconds might be too long for some data
- Consider making TTL configurable per query type

## Overall Assessment
⚠️ **Needs changes before merge** - Add migration file
```

## Example 4: Documentation Update Review

### Pull Request: Update API Documentation

#### Review Report:
```markdown
# Code Review Report

## Summary
This PR updates the API documentation to reflect recent changes and adds examples for all endpoints. Comprehensive and well-organized documentation update.

## 🚨 High Priority Issues
None found

## 💡 Suggestions

- Consider adding OpenAPI/Swagger specification
- Include authentication examples
- Add error response examples

## ✨ Nitpicks

**docs/api.md**
- Some code examples lack syntax highlighting
- Consider adding a quick start section

## Overall Assessment
🎉 **Excellent** - Ready to merge
```

## Usage Examples

### Using the Review Analyzer Script

```bash
# Analyze the last commit
python scripts/review-analyzer.py

# Analyze a specific commit range
python scripts/review-analyzer.py --range HEAD~3..HEAD

# Analyze a different repository
python scripts/review-analyzer.py --repo /path/to/repo

# Output JSON format
python scripts/review-analyzer.py --format json

# Save report to file
python scripts/review-analyzer.py --output review-report.md
```

### Using the Review Template

1. Copy the template from `templates/review-template.md`
2. Fill in each section based on your analysis
3. Use the script to automatically detect issues
4. Combine automated analysis with manual review

### Integration with Git Hooks

Add to `.git/hooks/pre-push`:
```bash
#!/bin/bash
# Run automated review before pushing
python scripts/review-analyzer.py --range origin/main..HEAD
if [ $? -ne 0 ]; then
    echo "Review found issues - please address before pushing"
    exit 1
fi
```

## Review Checklist

### Before Review
- [ ] Understand the purpose of the change
- [ ] Read the PR description carefully
- [ ] Check if tests are included
- [ ] Run the code locally if possible

### During Review
- [ ] Check for correctness and logic
- [ ] Verify security best practices
- [ ] Assess performance impact
- [ ] Review error handling
- [ ] Check test coverage
- [ ] Verify documentation

### After Review
- [ ] Provide clear, actionable feedback
- [ ] Explain the "why" behind suggestions
- [ ] Acknowledge good work
- [ ] Set clear approval conditions