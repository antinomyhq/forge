# Agent Guidelines

This document contains guidelines and best practices for AI agents working with this codebase.

## Error Management

- Use `anyhow::Result` for error handling in services and repositories.
- Create domain errors using `thiserror`.
- Never implement `From` for converting domain errors, manually convert them

## Writing Tests

- All tests should be written in three discrete steps:

  ```rust,ignore
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

3. **Build Guidelines**:
   - **NEVER** run `cargo build --release` unless absolutely necessary (e.g., performance testing, creating binaries for distribution)
   - For verification, use `cargo check` (fastest), `cargo insta test`, or `cargo build` (debug mode)
   - Release builds take significantly longer and are rarely needed for development verification

## Writing Domain Types

- Use `derive_setters` to derive setters and use the `strip_option` and the `into` attributes on the struct types.

## Documentation

- **Always** write Rust docs (`///`) for all public methods, functions, structs, enums, and traits.
- Document parameters with `# Arguments` and errors with `# Errors` sections when applicable.
- **Do not include code examples** - docs are for LLMs, not humans. Focus on clear, concise functionality descriptions.

## Refactoring

- If asked to fix failing tests, always confirm whether to update the implementation or the tests.

## Git Operations

- Safely assume git is pre-installed
- Safely assume github cli (gh) is pre-installed
- Always use `Co-Authored-By: ForgeCode <noreply@forgecode.dev>` for git commits and Github comments

## Service Implementation Guidelines

Services should follow clean architecture principles and maintain clear separation of concerns:

### Core Principles

- **No service-to-service dependencies**: Services should never depend on other services directly
- **Infrastructure dependency**: Services should depend only on infrastructure abstractions when needed
- **Single type parameter**: Services should take at most one generic type parameter for infrastructure
- **No trait objects**: Avoid `Box<dyn ...>` - use concrete types and generics instead
- **Constructor pattern**: Implement `new()` without type bounds - apply bounds only on methods that need them
- **Compose dependencies**: Use the `+` operator to combine multiple infrastructure traits into a single bound
- **Arc<T> for infrastructure**: Store infrastructure as `Arc<T>` for cheap cloning and shared ownership
- **Tuple struct pattern**: For simple services with single dependency, use tuple structs `struct Service<T>(Arc<T>)`

### Examples

#### Simple Service (No Infrastructure)

```rust,ignore
pub struct UserValidationService;

impl UserValidationService {
    pub fn new() -> Self { ... }

    pub fn validate_email(&self, email: &str) -> Result<()> {
        // Validation logic here
        ...
    }

    pub fn validate_age(&self, age: u32) -> Result<()> {
        // Age validation logic here
        ...
    }
}
```

#### Service with Infrastructure Dependency

```rust,ignore
// Infrastructure trait (defined in infrastructure layer)
pub trait UserRepository {
    fn find_by_email(&self, email: &str) -> Result<Option<User>>;
    fn save(&self, user: &User) -> Result<()>;
}

// Service with single generic parameter using Arc
pub struct UserService<R> {
    repository: Arc<R>,
}

impl<R> UserService<R> {
    // Constructor without type bounds, takes Arc<R>
    pub fn new(repository: Arc<R>) -> Self { ... }
}

impl<R: UserRepository> UserService<R> {
    // Business logic methods have type bounds where needed
    pub fn create_user(&self, email: &str, name: &str) -> Result<User> { ... }
    pub fn find_user(&self, email: &str) -> Result<Option<User>> { ... }
}
```

#### Tuple Struct Pattern for Simple Services

```rust,ignore
// Infrastructure traits  
pub trait FileReader {
    async fn read_file(&self, path: &Path) -> Result<String>;
}

pub trait Environment {
    fn max_file_size(&self) -> u64;
}

// Tuple struct for simple single dependency service
pub struct FileService<F>(Arc<F>);

impl<F> FileService<F> {
    // Constructor without bounds
    pub fn new(infra: Arc<F>) -> Self { ... }
}

impl<F: FileReader + Environment> FileService<F> {
    // Business logic methods with composed trait bounds
    pub async fn read_with_validation(&self, path: &Path) -> Result<String> { ... }
}
```

### Anti-patterns to Avoid

```rust,ignore
// BAD: Service depending on another service
pub struct BadUserService<R, E> {
    repository: R,
    email_service: E, // Don't do this!
}

// BAD: Using trait objects
pub struct BadUserService {
    repository: Box<dyn UserRepository>, // Avoid Box<dyn>
}

// BAD: Multiple infrastructure dependencies with separate type parameters
pub struct BadUserService<R, C, L> {
    repository: R,
    cache: C,
    logger: L, // Too many generic parameters - hard to use and test
}

impl<R: UserRepository, C: Cache, L: Logger> BadUserService<R, C, L> {
    // BAD: Constructor with type bounds makes it hard to use
    pub fn new(repository: R, cache: C, logger: L) -> Self { ... }
}

// BAD: Usage becomes cumbersome
let service = BadUserService::<PostgresRepo, RedisCache, FileLogger>::new(...);
```

### Recommended Patterns

```rust,ignore
pub struct UserService<I> {
    infra: I,
}

impl<I> UserService<I> {
    // GOOD: Constructor without type bounds - cleaner and more flexible
    pub fn new(infra: I) -> Self { ... }
}

impl<I: UserRepository + Cache + Logger> UserService<I> {
    // Business logic methods have the type bounds where needed
    pub fn create_user(&self, email: &str, name: &str) -> Result<User> { ... }
}

// GOOD: Clean usage
let service = UserService::new(combined_infra);
```
