---
layout: page
title: Development Guidelines
nav_order: 3
description: "Development guidelines for Code-Forge"
permalink: /guidelines
---

## Handling Errors

- Use `anyhow::Result` for error handling in services and repositories.
- Create domain errors using `thiserror`.
- Never implement `From` for converting domain errors, manually convert them
-

## Writing Tests

- All tests should be written in three discrete steps:

  ```rust
  use pretty_assertions::assert_eq; // Always use pretty assertions

  fn test_foo() {
      let fixture = ...; // Instantiate a fixture for the test
      let actual = ...; // Use the fixture to write a test
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
