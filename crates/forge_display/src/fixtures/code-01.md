## Permission Checking Flow for Fetch Requests

Based on the codebase analysis, here's where permissions are checked before making a fetch request:

### **Flow Overview:**

1. **Entry Point**: `crates/forge_app/src/tool_executor.rs:336`

   ```rust
   if env.enable_permissions && self.check_tool_permission(&tool_input, context).await?
   ```

2. **Permission Check Method**: `crates/forge_app/src/tool_executor.rs:48-72`
   - The `check_tool_permission()` method is called before executing any tool
   - It converts the tool catalog to a policy operation

3. **Policy Operation Conversion**: `crates/forge_domain/src/tools/catalog.rs:680-684`

   ```rust
   ToolCatalog::Fetch(input) => Some(crate::policies::PermissionOperation::Fetch {
       url: input.url.clone(),
       cwd,
       message: format!("Fetch content from URL: {}", input.url),
   })
   ```

4. **Permission Decision**: `crates/forge_services/src/policy.rs:163-208`
   - The `check_operation_permission()` method evaluates the fetch operation against policies
   - Uses `PolicyEngine::can_perform()` to check rules

5. **Rule Matching**: `crates/forge_domain/src/policies/rule.rs:88-96`

   ```rust
   (Rule::Fetch(rule), PermissionOperation::Fetch { url, cwd, message: _ }) => {
       let url_matches = match_pattern(&rule.url, url);
       let dir_matches = match &rule.dir {
           Some(wd_pattern) => match_pattern(wd_pattern, cwd),
           None => true,
       };
       url_matches && dir_matches
   }
   ```

6. **Actual Fetch Execution**: `crates/forge_app/src/tool_executor.rs:282-284`
   - Only executed if permission is granted
   ```rust
   ToolCatalog::Fetch(input) => {
       let output = self.services.fetch(input.url.clone(), input.raw).await?;
       (input, output).into()
   }
   ```

### **Key Points:**

- **Gating Condition**: Permissions are only checked if `env.enable_permissions` is true
- **Permission Denial**: If denied, returns a "Permission Denied" error without executing the fetch
- **Policy Types**: Can be `Allow`, `Deny`, or `Confirm` (prompts user)
- **Pattern Matching**: Fetch rules match against URL patterns (e.g., `"https://api.example.com/*"`)
- **User Confirmation**: If no policy matches, the user is prompted to Allow, Deny, or Remember the decision

The permission check is a **gating mechanism** that prevents the actual HTTP fetch from occurring unless explicitly allowed by the policy engine.
