# Model Selection Command Implementation

## Objective
Add a `/model` command to the Forge CLI that uses the `inquire` library to display a list of available models and update the project's `forge.yaml` file with the selected model.

## Implementation Plan
- [x] Add `inquire` as a dependency to the workspace and forge_main crate
- [x] Implement a `handle_model_selection()` method in the UI module to:
  - Fetch available models using the API
  - Display a selection interface using inquire
  - Update the forge.yaml file with the selected model
- [x] Update the Command::Model match case to call the new method

## Implementation Details
1. Added `inquire` dependency to the workspace and forge_main Cargo.toml files
2. Implemented a new `handle_model_selection()` method in the UI module that:
   - Fetches the list of available models
   - Uses inquire to display a selection list
   - Updates the standard_model anchor in the forge.yaml file
3. Updated the Command::Model case in the UI::run method to call the new method

## Verification Criteria
- The `/model` command should display a list of available models using inquire
- After selecting a model, the forge.yaml file should be updated with the selected model
- The standard_model anchor in the forge.yaml file should be created if it doesn't exist
- Appropriate error messages should be displayed if any step fails

## Potential Risks and Mitigations
- Risk: Models API might return an empty list
  Mitigation: Added error handling for empty model lists
- Risk: forge.yaml file might not exist in the current directory
  Mitigation: Added error handling for missing files
- Risk: forge.yaml file might be in an unexpected format
  Mitigation: Added error handling for parsing errors

## Future Improvements
- Add support for updating advanced_model in addition to standard_model
- Add support for creating forge.yaml if it doesn't exist
- Add ability to filter models by provider