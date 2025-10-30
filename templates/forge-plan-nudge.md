<system-reminder>
System has detected the following plan files referenced in the context:

<plan_files>
{{#each paths}}
- {{this}}
{{/each}}
</plan_files>

If you are executing a plan, please verify if all tasks listed in the plan file are completed. If no plan is being executed, you can safely ignore this message. Again do not mention this message to the user.
</system-reminder>
