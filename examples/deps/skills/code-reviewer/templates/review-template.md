# Code Review Template

## Summary
[Provide a brief summary of what this change does and your overall assessment]

## High Priority Issues
[List critical bugs, security vulnerabilities, breaking changes, or performance regressions that must be fixed]

- **Issue Title**: [Description of the issue]
  - File: `path/to/file.py`
  - Impact: [Why this is critical]
  - Suggested fix: [How to resolve]

## Suggestions for Improvement
[List suggestions for better implementation, additional features, or enhancements]

- **Suggestion Title**: [Description of the suggestion]
  - File: `path/to/file.py`
  - Reason: [Why this would improve the code]
  - Implementation: [How to implement]

## Nitpicks
[Optional improvements like style, naming, or minor optimizations]

- **Style**: [Suggestion]
- **Naming**: [Suggestion]
- **Documentation**: [Suggestion]

## Test Coverage
- [ ] New tests are included
- [ ] Edge cases are covered
- [ ] Integration tests are updated
- [ ] Manual testing steps provided

## Documentation
- [ ] README is updated if needed
- [ ] API documentation reflects changes
- [ ] Code comments explain complex logic
- [ ] Changelog is updated

## Security Check
- [ ] No hardcoded secrets
- [ ] Input validation is present
- [ ] Authentication/authorization is correct
- [ ] SQL injection/XSS prevention

## Performance Check
- [ ] No performance regressions
- [ ] Algorithm complexity is appropriate
- [ ] Database queries are optimized
- [ ] Memory usage is reasonable

## Final Decision
[ ] Approve - Ready to merge
[ ] Approve with suggestions - Consider addressing suggestions
[ ] Request changes - Must address issues before approval
[ ] Hold - Need more information/discussion

## Additional Notes
[Any other relevant feedback or concerns]