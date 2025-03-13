# Custom Command Dispatch Feature

This PR adds support for custom dispatch commands using the `/dispatch-event_name value` syntax, as requested in issue #409.

## Overview

The feature allows users to use a standardized format for custom commands that can trigger custom event handling. The format follows:

```
/dispatch-event_name value
```

Where:
- `event_name` is the name of the event to dispatch
- `value` is the value/payload for that event (can be empty)

## Examples

Here are some examples of how to use the feature:

```
/dispatch-github create an issue to update tokio version
```
This would dispatch an event with name "github" and value "create an issue to update tokio version".

```
/dispatch-notification
```
This would dispatch an event with name "notification" and an empty value.

```
/dispatch-log-error Error message with  spaces
```
This would dispatch an event with name "log-error" and value "Error message with  spaces".

## Implementation Details

1. Added a new `Command::Dispatch(String, String)` variant to the `Command` enum
2. Updated the command parser to recognize the "/dispatch-" prefix 
3. Added a new `dispatch` method to the `API` trait
4. Implemented the method in `ForgeAPI`
5. Updated the UI to handle the new command variant

## Testing

The feature has been tested with various combinations of event names and values, including empty values.

## Notes for Event Handlers

When implementing event handlers for these custom events, be aware that:
- Event names can be any string (though they should generally be alphanumeric with hyphens/underscores for readability)
- Values may be empty
- Values preserve whitespace exactly as entered
