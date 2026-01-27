**Execution Interrupted**

**Reason:** Maximum tool failure limit reached
**Limit:** {{limit}} failures per turn
**Failed Tools:**
{{#each errors}}
  - {{@key}} failed {{this}} time(s)
{{/each}}

The agent was unable to complete the task due to repeated tool failures. Consider:
- Simplifying the task or breaking it into smaller steps
- Checking if the required resources or permissions are available
- Trying a different approach or using different tools
