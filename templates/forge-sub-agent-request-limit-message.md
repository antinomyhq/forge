**Sub-agent Execution Interrupted**

**Reason:** Maximum request per turn limit reached
**Limit:** {{limit}} requests per turn
**Requests Made:** {{request_count}}

The sub-agent was unable to complete the task within the allowed number of requests. This typically happens when:
- The task is too complex and requires many iterations
- The agent is stuck in a loop or making repeated unsuccessful attempts
- Multiple tool calls are needed that exceed the limit

Consider:
- Breaking the task into smaller, more focused sub-tasks
- Simplifying the task requirements
- Providing more specific instructions to reduce trial-and-error