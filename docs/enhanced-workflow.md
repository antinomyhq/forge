# Using the Enhanced Hybrid Workflow

The enhanced hybrid workflow provides a structured way to develop and implement solutions using Forge, with a clearly separated planning and implementation phase. This document explains how to use this workflow effectively.

## Workflow Overview

The enhanced hybrid workflow consists of the following stages:

1. **Issue Planning**: When an issue is labeled with `forge-automate`, a plan is automatically created
2. **Plan Review**: The plan can be reviewed, revised, and ultimately approved
3. **Implementation**: Once approved, the plan is implemented incrementally
4. **Code Review**: Feedback can be provided on the implementation and will be automatically addressed

## How to Use the Enhanced Workflow

### 1. Starting the Process

To start the process, simply add the `forge-automate` label to a GitHub issue. This will:

- Trigger the plan creation process
- Generate a draft PR with a detailed implementation plan in a `.task-{issue_number}.md` file
- Label the PR with `forge-plan-review` to indicate it's ready for review

### 2. Reviewing and Revising the Plan

Once the plan is created, you can:

#### Request Changes to the Plan

If you want the plan to be revised, you have two options:

**Option A**: Comment on the PR with the following format:
```
/forge revise-plan

The current approach needs to be changed because...
Consider using a different approach that...
Also add tests for...
```

**Option B**: Use GitHub's review functionality to request changes directly to the `.task` file

#### Approve the Plan

When you're satisfied with the plan, approve it by commenting:
```
/forge approve-plan
```

This will:
- Remove the `forge-plan-review` label
- Add the `forge-implement` label
- Keep the PR in draft state but mark it as ready for implementation

### 3. Implementation

Once the plan is approved, implementation will begin automatically, or you can trigger it manually:

```
/forge continue
```

The implementation will proceed incrementally with:
- Small, focused commits
- Progress updates as comments on the PR
- The PR will be marked as ready for review when complete

### 4. Addressing Review Comments

When the implementation is complete and you review the PR:
- Any review comments will be automatically processed
- The code will be updated to address feedback
- You'll receive responses to your feedback

## Labels and Their Meanings

- **forge-automate**: Triggers the entire automated process
- **forge-plan-review**: Indicates the plan is ready for review but not yet approved
- **forge-implement**: Indicates the plan is approved and ready for implementation

## Commands

- **/forge revise-plan**: Request changes to the plan
- **/forge approve-plan**: Approve the plan and prepare for implementation
- **/forge continue**: Manually trigger implementation after plan approval

## Example Workflow

1. Add `forge-automate` label to issue #42
2. A draft PR is created with a plan in `.task-42.md`
3. You review the plan and comment `/forge revise-plan` with feedback
4. The plan is updated and you comment `/forge approve-plan`
5. Implementation begins automatically
6. When complete, you review the PR and leave comments
7. The comments are addressed automatically
8. You approve the PR and merge it