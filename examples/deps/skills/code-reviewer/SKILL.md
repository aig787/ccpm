---
name: code-reviewer
description: Perform comprehensive code reviews with focus on correctness, performance, security, and maintainability. Use when reviewing pull requests, merge requests, or code changes.
---

# Code Reviewer

## Instructions

When performing code reviews, follow this structured approach:

### 1. Initial Assessment
- Understand the purpose and context of the changes
- Check if the change matches the description/commit message
- Verify the change solves the intended problem
- Look for unintended side effects

### 2. Review Checklist

#### Correctness
- [ ] Logic is sound and achieves the intended purpose
- [ ] Edge cases are handled appropriately
- [ ] Error handling is comprehensive
- [ ] No obvious bugs or logical flaws
- [ ] Tests cover the new/modified code paths

#### Performance
- [ ] Algorithm complexity is appropriate
- [ ] No unnecessary computations or redundant operations
- [ ] Resource usage (memory, CPU) is efficient
- [ ] Database queries are optimized (if applicable)
- [ ] Caching is used where beneficial

#### Security
- [ ] Input validation and sanitization
- [ ] No hardcoded secrets or sensitive data
- [ ] Proper authentication/authorization checks
- [ ] SQL injection and XSS prevention
- [ ] Secure handling of file uploads/downloads

#### Maintainability
- [ ] Code is clear and readable
- [ ] Variable/function names are descriptive
- [ ] Comments explain complex logic, not obvious code
- [ ] Follows project's coding standards
- [ ] No dead code or commented-out code

#### Testing
- [ ] Unit tests are provided for new functionality
- [ ] Test cases cover normal and edge cases
- [ ] Integration tests are updated if needed
- [ ] No tests are broken by this change

### 3. Review Format

#### Overall Summary
Start with a brief summary of what the change does and your overall assessment.

#### Detailed Feedback
Organize feedback by category:

**High Priority Issues** (Must fix before merge):
- Critical bugs
- Security vulnerabilities
- Breaking changes
- Performance regressions

**Suggestions for Improvement** (Nice to have):
- Code clarity improvements
- Better error handling
- Performance optimizations
- Additional test coverage

**Nitpicks** (Optional improvements):
- Style/formatting issues
- Variable naming suggestions
- Code organization tips

#### Example Review Structure
```markdown
## Summary
This PR adds user authentication with JWT tokens. The implementation is solid but has a few security concerns that should be addressed.

## High Priority Issues
1. **Security**: JWT secret is hardcoded in `config.rs`. Move to environment variable.
2. **Error Handling**: The `/login` endpoint doesn't rate limit failed attempts.

## Suggestions
1. Consider adding refresh tokens for better security.
2. The password validation could be strengthened with additional complexity requirements.

## Nitpicks
1. Some function names could be more descriptive (e.g., `auth()` → `authenticate_user()`).
2. Add inline comments explaining the JWT validation logic.
```

### 4. Best Practices for Reviewers
- Be constructive and respectful
- Explain why something is an issue, not just that it is
- Provide specific examples and suggest solutions
- Ask questions if you don't understand something
- Recognize good work and positive aspects
- Focus on the code, not the author

### 5. Special Considerations

#### Breaking Changes
- Clearly identify any breaking API changes
- Ensure migration path is documented
- Check if dependent code needs updates

#### Dependencies
- Review new dependency additions carefully
- Check for security vulnerabilities in dependencies
- Verify dependency licenses are compatible

#### Documentation
- README should be updated for user-facing changes
- API documentation should reflect new endpoints
- Code comments should explain complex algorithms

### 6. Automated Checks
Before reviewing, ensure:
- CI pipeline passes
- Code formatting follows project standards
- Linting rules are satisfied
- Test coverage is maintained or improved

### 7. Final Decision
After review, provide clear approval status:
- **Approve**: Ready to merge as-is
- **Approve with suggestions**: Ready to merge, but consider suggestions
- **Request changes**: Must address issues before approval
- **Hold**: Waiting for clarification or additional context

## Usage

1. Open the pull request or merge request
2. Run this skill to analyze the changes
3. Provide feedback following the structured format
4. Engage in discussion to resolve issues
5. Approve when all concerns are addressed

## Tips

- Start by running the code locally to verify it works
- Check both the changed code and related code that might be affected
- Consider the impact on end users and downstream systems
- Remember that perfect code doesn't exist – focus on improvement