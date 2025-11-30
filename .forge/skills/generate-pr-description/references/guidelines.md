# PR Description Guidelines

## Writing Principles

### 1. Lead with Impact
Start with the most important information that reviewers need to know:
- What problem does this solve?
- What's the scope of changes?
- Are there any risks or breaking changes?

### 2. Write for Your Audience
Consider who will be reading:
- **Technical reviewers**: Need implementation details and architectural context
- **Product managers**: Need feature impact and user value
- **QA teams**: Need testing scenarios and edge cases
- **Future maintainers**: Need context for why decisions were made

### 3. Structure for Scanning
Use formatting that makes information easy to find:
- Headers and subheaders for organization
- Bullet points for lists and changes
- Code blocks for examples
- Checkboxes for completed items

## Content Guidelines

### Essential Information
Every PR description should include:

**Purpose**: Why this change exists
- Link to issue, feature request, or bug report
- Business or technical motivation
- User impact or value delivered

**Scope**: What's included and what's not
- High-level summary of changes
- Components or areas affected
- Boundaries of the change

**Approach**: How the solution works
- Key architectural decisions
- Trade-offs considered
- Alternative approaches evaluated

**Testing**: How changes were validated
- Test scenarios covered
- Manual testing performed
- Regression testing done

### Optional but Valuable
Include when relevant:

**Migration Notes**: For database changes, API changes, or configuration updates
**Performance Impact**: For changes affecting system performance
**Security Considerations**: For changes touching authentication, authorization, or data handling
**Dependencies**: For changes requiring other teams' work or external updates
**Rollback Plan**: For risky changes or production deployments

## Quality Standards

### Clarity Checklist
- [ ] Purpose is clear from the title and first paragraph
- [ ] Technical terms are explained or linked to documentation
- [ ] Code examples are properly formatted and tested
- [ ] Screenshots or diagrams are included for UI changes
- [ ] Links to related issues, docs, or PRs are working

### Completeness Checklist
- [ ] All significant changes are documented
- [ ] Testing approach is described
- [ ] Breaking changes are clearly marked
- [ ] Migration steps are provided (if needed)
- [ ] Review focus areas are identified

### Conciseness Checklist
- [ ] No redundant information
- [ ] Implementation details are summarized, not exhaustive
- [ ] Focus on "what" and "why" over "how"
- [ ] Bullet points used instead of long paragraphs

## Team-Specific Adaptations

### For Frontend Teams
Include additional sections:
- **UI/UX Impact**: Screenshots, design mockups, accessibility considerations
- **Browser Compatibility**: Testing across different browsers and devices
- **Performance Metrics**: Bundle size impact, rendering performance, Core Web Vitals
- **Design Review**: Approval from design team, adherence to design system

### For Backend Teams
Include additional sections:
- **API Changes**: Endpoint modifications, versioning considerations, backward compatibility
- **Database Impact**: Migration scripts, performance implications, data integrity
- **Infrastructure**: Deployment requirements, environment variable changes, resource needs
- **Monitoring**: New metrics, alerts, logging enhancements

### For DevOps Teams
Include additional sections:
- **Infrastructure Changes**: Resource modifications, networking changes, security updates
- **Deployment Process**: Special deployment steps, rollback procedures, monitoring requirements
- **Service Dependencies**: Impact on other services, coordination requirements
- **Compliance**: Security, audit, or regulatory considerations

### For Mobile Teams
Include additional sections:
- **Platform Considerations**: iOS/Android specific changes, version compatibility
- **App Store**: Changes affecting app review, privacy policy updates
- **Performance**: Battery usage, memory impact, network efficiency
- **User Experience**: Navigation changes, accessibility improvements

## Review Process Integration

### Pre-Review Checklist
Before requesting review:
- [ ] Description is complete and accurate
- [ ] All checkboxes in "Changes" section are completed
- [ ] Links to related issues/docs are included
- [ ] Testing section describes validation approach
- [ ] Any special review instructions are noted

### Reviewer Guidance
Help reviewers focus their attention:
- **Critical Areas**: Highlight code that needs careful review
- **Context**: Explain unfamiliar domain concepts or business logic
- **Dependencies**: Note any changes that affect other teams
- **Testing Gaps**: Areas where additional testing might be valuable

### Post-Review Updates
When addressing feedback:
- Update description if significant changes are made
- Document any architectural decisions that changed
- Add notes about new testing performed
- Update migration notes if deployment process changes

## Common Mistakes to Avoid

### Content Issues
- **Generic descriptions**: "Fixed bugs and added features"
- **Missing context**: Not explaining why changes were needed
- **Implementation focus**: Too much detail about how code works
- **Incomplete testing**: Not describing validation approach

### Formatting Issues
- **Wall of text**: Long paragraphs without structure
- **Missing formatting**: No headers, bullets, or code blocks
- **Broken links**: References to issues or docs that don't work
- **Inconsistent style**: Mixing different formatting approaches

### Process Issues
- **Stale descriptions**: Not updating when PR changes significantly
- **Missing updates**: Not documenting post-review changes
- **Wrong audience**: Writing only for yourself or only for others
- **Premature publishing**: Requesting review before description is complete

## Continuous Improvement

### Metrics to Track
- **Review time**: How long does it take reviewers to understand changes?
- **Review questions**: What do reviewers consistently ask about?
- **Deployment issues**: What problems arise from missing information?
- **Documentation gaps**: What context is missing for future maintainers?

### Regular Review
Periodically evaluate PR description quality:
- Review recent PRs for completeness and clarity
- Gather feedback from team members on description usefulness
- Identify patterns in review comments that could be addressed in descriptions
- Update templates and guidelines based on lessons learned

### Team Standards
Establish team-specific guidelines:
- **Required sections**: What must be included in every PR
- **Review criteria**: What reviewers should check in descriptions
- **Template usage**: When to use specific templates
- **Quality gates**: Standards for approving PRs based on description quality

## Integration with Tools

### Issue Tracking
- Link to issues using consistent format (`Closes #123`, `Fixes #456`)
- Include issue titles for context
- Reference related issues even if not directly closed

### Documentation
- Link to relevant documentation pages
- Update docs as part of the PR when needed
- Include links to architectural decision records (ADRs)

### Monitoring and Analytics
- Reference relevant dashboards or metrics
- Include links to monitoring systems
- Document new metrics or alerts being added

### CI/CD Integration
- Explain any special build or deployment requirements
- Document environment variable changes
- Include rollback procedures for production changes

## Examples of Excellence

Look for these patterns in high-quality PR descriptions:

### Clear Problem Statement
```markdown
## Problem
Users report that search results are slow when filtering by multiple criteria.
Performance testing shows queries take 3-5 seconds with 3+ filters applied.
This affects 40% of daily active users based on analytics data.
```

### Focused Solution Description
```markdown
## Solution
Added database indexes on commonly filtered columns and implemented query optimization:
- Created composite index on (category, status, created_date)
- Rewrote filter logic to use single optimized query instead of multiple queries
- Added query result caching for frequently accessed data
```

### Comprehensive Testing Summary
```markdown
## Testing
### Performance Testing
- Query time reduced from 3-5s to 200-400ms (87% improvement)
- Tested with production-size dataset (2M records)
- Memory usage increased by <5% due to additional indexes

### Functional Testing
- All existing search functionality verified working
- Edge cases tested: empty filters, invalid criteria, very large result sets
- Cross-browser testing completed on Chrome, Firefox, Safari
```

### Actionable Review Notes
```markdown
## Review Focus
- **Query optimization logic** in `search_service.rs:45-78` - complex indexing strategy
- **Database migration** in `migrations/001_add_search_indexes.sql` - verify index choices
- **Caching implementation** - ensure cache invalidation handles all update scenarios
```