# Agent Guidelines

This document contains guidelines and best practices for AI agents working with this codebase.

## Error Management

- Use `anyhow::Result` for error handling in services and repositories.
- Create domain errors using `thiserror`.
- Never implement `From` for converting domain errors, manually convert them

## Writing Tests

- All tests should be written in three discrete steps:

  ```rust
  use pretty_assertions::assert_eq; // Always use pretty assertions

  fn test_foo() {
      let fixture = ...; // Instantiate a fixture for the test
      let actual = ...; // Execute the fixture to create an output
      let expected = ...; // Define a hand written expected result
      assert_eq!(actual, expected); // Assert that the actual result matches the expected result
  }
  ```

- Use `pretty_assertions` for better error messages.

- Use fixtures to create test data.

- Use `assert_eq!` for equality checks.

- Use `assert!(...)` for boolean checks.

- Use unwraps in test functions and anyhow::Result in fixtures.

- Keep the boilerplate to a minimum.

- Use words like `fixture`, `actual` and `expected` in test functions.

- Fixtures should be generic and reusable.

- Test should always be written in the same file as the source code.

- We use `insta` to run tests:
  ```
  cargo insta test --accept --unreferenced=delete
  ```
- Use `new`, Default and derive_setters::Setters to create `actual`, `expected` and specially `fixtures`. For eg:
  Good
  User::default().age(12).is_happy(true).name("John")
  User::new("Job").age(12).is_happy()
  User::test() // Special test constructor

  Bad
  Use {name: "John".to_string(), is_happy: true, age: 12}
  User::with_name("Job") // Bad name, should stick to User::new() or User::test()

- Use unwrap() unless the error information is useful. Use `expect` instead of `panic!` when error message is useful for eg:
  Good
  users.first().expect("List should not be empty")

  Bad
  if let Some(user) = users.first() {
  // ...
  } else {
  panic!("List should not be empty")
  }

- Prefer using assert_eq on full objects instead of asserting each field
  Good
  assert_eq(actual, expected);

  Bad
  assert_eq(actual.a, expected.a);
  assert_eq(actual.b, expected.b);

## Verification

Always verify changes by running tests and linting the codebase

1. Run crate specific tests to ensure they pass.

   ```
   cargo insta test --accept --unreferenced=delete
   ```

2. Lint and format the codebase.
   ```
   cargo +nightly fmt --all && cargo +nightly clippy --fix --allow-staged --allow-dirty --workspace;
   ```

## Writing Domain Types

- Use `derive_setters` to derive setters and use the `strip_option` and the `into` attributes on the struct types.

## Refactoring

- If asked to fix failing tests, always confirm whether to update the implementation or the tests.

## Elm Architecture (in forge_main_neo)

- Command executors should ALWAYS return Option<Action>, never send them directly through channels
- Actions are the only way to update application state
- State updates trigger UI changes through the unidirectional data flow
- Commands represent intent to perform side effects
- Actions represent the result of those side effects
- The executor pattern: Command -> Side Effect -> Action -> State Update -> UI Update

## Git Operations

- Safely assume git is pre-installed
- Safely assume github cli (gh) is pre-installed
- Always use `Co-Authored-By: ForgeCode <noreply@forgecode.dev>` for git commits and Github comments
