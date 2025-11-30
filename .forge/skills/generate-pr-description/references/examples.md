# PR Description Examples

## Example 1: Feature Addition

```markdown
## What
Add user authentication with JWT tokens and session management

## Why
**Problem**: Users currently cannot save preferences or access personalized content
**Impact**: Enables personalized user experience and secure data access

## How
**Approach**: Implement JWT-based authentication with refresh token rotation
**Key Components**: Auth service, JWT middleware, user session management

## Changes
### Core Features
- [x] JWT token generation and validation
- [x] User login/logout endpoints
- [x] Session middleware for protected routes
- [x] Password hashing with bcrypt

### Supporting Changes
- [x] User model with password field
- [x] Authentication tests (unit + integration)
- [x] API documentation updated
- [x] Error handling for auth failures

## Testing
### Unit Tests
- [x] JWT token validation logic
- [x] Password hashing/verification
- [x] Session middleware behavior

### Integration Tests
- [x] Login/logout flow end-to-end
- [x] Protected route access control
- [x] Token refresh mechanism

### Manual Testing
- [x] Happy path: successful login/logout
- [x] Error scenarios: invalid credentials, expired tokens
- [x] Browser session persistence

## Security Considerations
- JWT tokens expire after 1 hour with refresh rotation
- Passwords hashed with bcrypt (12 rounds)
- Rate limiting on auth endpoints (5 attempts/minute)
- Secure HTTP-only cookies for token storage

## Migration Notes
- New `users` table created automatically
- No existing data migration required
- Environment variable `JWT_SECRET` must be set before deployment
```

## Example 2: Bug Fix

```markdown
## What
Fix for Issue #247: Prevent memory leak in file upload processing

## Problem
**Root Cause**: File upload streams weren't being properly closed after processing
**Impact**: Server memory usage grew continuously with each upload
**Reproduction**: Upload multiple large files (>10MB) and observe memory growth

## Solution
**Approach**: Ensure proper cleanup of file streams and temporary files
**Changes**: Added try-finally blocks and explicit stream closing

## Changes
### Bug Fix
- [x] Fixed file stream cleanup in upload handler
- [x] Added automatic temp file deletion
- [x] Improved error handling for partial uploads

### Prevention
- [x] Added integration test for memory usage
- [x] Enhanced monitoring for file processing
- [x] Added cleanup verification logs

## Testing
### Regression Tests
- [x] Memory usage stable during repeated uploads
- [x] Large file uploads complete successfully
- [x] Error scenarios don't leak resources

### Manual Verification
- [x] 50+ file uploads show stable memory usage
- [x] Server restart no longer needed after heavy usage
- [x] Monitoring dashboard shows healthy metrics

## Validation
- [x] Issue reporter (@user123) confirmed fix in staging
- [x] QA team verified with load testing
- [x] Production monitoring shows improvement

## Risk Assessment
**Low**: Isolated change to file handling with comprehensive testing
**Mitigation**: Gradual rollout with enhanced monitoring
```

## Example 3: Refactor

```markdown
## What
Refactor authentication middleware: Extract reusable components and improve testability

## Why
**Technical Debt**: Authentication logic was scattered across multiple files
**Benefits**: Improved code organization, easier testing, better error handling
**Future Enablement**: Prepares for multi-factor authentication implementation

## How
**Strategy**: Extract authentication logic into dedicated service classes
**Scope**: Middleware, auth service, token validation - no API changes

## Changes
### Code Structure
- [x] Created `AuthenticationService` class
- [x] Extracted `TokenValidator` utility
- [x] Consolidated error handling in `AuthError` types
- [x] Moved auth middleware to dedicated module

### Technical Improvements
- [x] Removed code duplication (3 similar auth checks → 1 service)
- [x] Improved error messages with specific failure reasons
- [x] Added proper type safety for auth context
- [x] Simplified middleware configuration

### Maintenance
- [x] Updated inline documentation
- [x] Added architectural decision record (ADR)
- [x] Removed deprecated auth utility functions

## Testing
### Behavior Preservation
- [x] All existing tests pass without modification
- [x] API behavior unchanged (confirmed with integration tests)
- [x] Performance benchmarks show 5% improvement

### Code Quality
- [x] Lint passes with zero warnings
- [x] Code coverage increased from 78% to 85%
- [x] Complexity metrics improved (cyclomatic complexity reduced)

## Notes
**No Functional Changes**: This is purely a refactoring - no user-facing behavior changes
**Review Focus**: Code organization, service boundaries, and error handling patterns
**Follow-up**: Next PR will add multi-factor authentication using new service structure
```

## Example 4: Breaking Changes

```markdown
## ⚠️ BREAKING CHANGES: API v2.0 Migration

## What
User API v2.0: Standardized response format and improved error handling

## Breaking Changes
### API Changes
- **Removed**: `/users` endpoint (use `/api/v2/users` instead)
- **Changed**: User response format now includes metadata wrapper
- **Added**: Required `Content-Type: application/json` header for all requests

### Response Format Changes
```json
// Old format (v1)
{
  "id": 123,
  "name": "John Doe",
  "email": "john@example.com"
}

// New format (v2)
{
  "data": {
    "id": 123,
    "name": "John Doe", 
    "email": "john@example.com"
  },
  "meta": {
    "version": "2.0",
    "timestamp": "2024-01-15T10:30:00Z"
  }
}
```

## Migration Guide
### For API Consumers
```bash
# Old way
curl -X GET /users/123

# New way  
curl -X GET /api/v2/users/123 -H "Content-Type: application/json"
```

### Code Updates Required
```javascript
// Old client code
const user = response.data;
const name = user.name;

// New client code  
const user = response.data.data;
const name = user.name;
```

## Timeline
- **Deprecation Notice**: December 1, 2023 (announced)
- **Breaking Change**: January 15, 2024 (this release)
- **v1 Support End**: March 1, 2024

## Backward Compatibility
- [x] Migration script provided (`scripts/migrate-api-calls.sh`)
- [x] Documentation updated with migration examples
- [x] Support team notified and trained
- [x] Monitoring added for v1 endpoint usage

## Rollback Plan
1. Revert API routes to v1 format
2. Update client applications to use legacy endpoints
3. Communicate rollback to all stakeholders
4. Estimated rollback time: 30 minutes

## Communication
- [x] Breaking change announced via email (Dec 1)
- [x] Migration guide published in docs
- [x] Engineering teams notified in #engineering Slack
- [x] Customer support scripts updated
```

## Example 5: Documentation

```markdown
## What
Documentation update: Complete API reference with examples and troubleshooting guide

## Why
**Need**: Developers were struggling with incomplete API documentation
**Audience**: External developers integrating with our API, internal team members

## Changes
### Content Updates
- [x] Added comprehensive endpoint documentation (15 new endpoints)
- [x] Included request/response examples for all methods
- [x] Added authentication guide with code samples
- [x] Created troubleshooting section with common error solutions

### Structure Improvements
- [x] Reorganized docs by feature area (Users, Orders, Payments)
- [x] Added quick-start guide for new developers
- [x] Created interactive API explorer integration
- [x] Added search functionality to docs site

### Examples Added
- [x] cURL examples for all endpoints
- [x] JavaScript/Python SDK usage examples
- [x] Webhook implementation guide
- [x] Rate limiting and pagination examples

## Quality Checks
- [x] Technical accuracy verified by API team
- [x] All code examples tested and working
- [x] Links tested (internal and external)
- [x] Content reviewed for clarity and completeness

## Review Notes
**Focus Areas**: Authentication flow documentation and error handling examples
**Target Audience**: Developers with varying API experience levels
**Follow-up**: Gather feedback from first 10 developers using new docs
```

## Example 6: Security Update

```markdown
## 🔒 Security Update: SQL Injection Prevention

## What
Security improvement: Implement parameterized queries and input validation

## Security Issues Addressed
- **Issue Type**: SQL Injection vulnerabilities in user search
- **Severity**: High
- **Impact**: All user search functionality, potential data exposure

## Changes
### Security Enhancements
- [x] Replaced string concatenation with parameterized queries
- [x] Added input validation for all search parameters
- [x] Implemented query whitelisting for allowed operations
- [x] Added rate limiting on search endpoints

### Monitoring & Logging
- [x] Security event logging for suspicious query patterns
- [x] Added alerting for potential injection attempts
- [x] Enhanced audit trail for all database queries

## Testing
### Security Testing
- [x] Penetration testing performed by security team
- [x] Automated vulnerability scan clean
- [x] Manual testing of common injection patterns

### Functional Testing
- [x] All search functionality verified working
- [x] Performance impact minimal (<2ms average increase)
- [x] User experience unchanged

## Deployment
### Security Considerations
- [x] Staged rollout planned (10% → 50% → 100%)
- [x] Enhanced monitoring for 48 hours post-deployment
- [x] Incident response team on standby

## Notes
**Confidential**: Specific vulnerability details shared separately with security team
**Contact**: Security team (@security-team) for sensitive information
**Compliance**: Addresses findings from Q4 security audit
```

## Common Patterns and Best Practices

### Effective Openings
- Lead with the most important information
- Use action verbs to describe what the PR accomplishes
- Be specific about the scope and impact

**Good Examples:**
- "Add user authentication with JWT tokens"
- "Fix memory leak in file upload processing"  
- "Refactor payment service for improved testability"

**Poor Examples:**
- "Some changes to the auth system"
- "Fixed a bug"
- "Updated some files"

### Clear Problem Statements
- Explain the "why" behind the change
- Include impact on users or system
- Reference specific issues or requirements

**Good Examples:**
- "Users cannot save preferences without authentication"
- "Memory usage grows continuously during file uploads"
- "Authentication code is scattered and hard to test"

### Focused Change Lists
- Group related changes logically
- Use checkboxes for completed items
- Highlight breaking changes or migrations

### Comprehensive Testing
- Cover both positive and negative test cases
- Include different types of testing (unit, integration, manual)
- Mention specific scenarios tested

### Actionable Notes
- Include reviewer guidance
- Mention deployment considerations
- Note any follow-up work needed

## Anti-Patterns to Avoid

### Vague Descriptions
- ❌ "Made some improvements"
- ❌ "Fixed issues"
- ❌ "Updated code"

### Technical Jargon Without Context
- ❌ "Refactored the service layer using DI container"
- ✅ "Refactored service layer to improve testability by using dependency injection"

### Missing Context
- ❌ Just listing files changed
- ❌ No explanation of why changes were needed
- ❌ No testing information

### Information Overload
- ❌ Including every single line change
- ❌ Too much implementation detail
- ❌ Irrelevant background information