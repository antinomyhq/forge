---
layout: page
title: Service Documentation
nav_order: 4
description: "Service implementation details for Code-Forge"
permalink: /service
---

## Example

The `Service` struct acts as a factory, providing static methods to create service instances:

```rust

#[async_trait::async_trait]
trait MyService: Send + Sync {

    // - Can be async and require Send + Sync
    // - If method can fail, it should return a `crate::Result`
    async fn my_method(&self) -> crate::Result<()>;
}


// Add associated functions to the Service struct
impl crate::Service {
    // Method name matches the service trait
    // It should be public, never fail and lazily initialized
    pub fn my_service() -> impl MyService {
        Live::new()
    }
}

// Live service is always private
struct Live {
    // Fields
}


impl Live {
    pub fn new() -> Self {
        Live {
            // Initialize fields
        }
    }
}

#[async_trait::async_trait]
impl MyService for Live {
    async fn my_method(&self) -> Result<()> {
        // Implementation
    }
}
```

Services include test implementations and utilities:

```rust
#[cfg(test)]
pub mod tests {
    pub struct TestMyService {
        // Fields
    }

    impl TestMyService {
        // Test service isn't exposed via crate::Service
        pub fn new() -> Self {
            TestMyService {
                // Initialize fields
            }
        }
    }
}
```

- Nomenclature: File names should match the service name for example ChatService should be in chat.rs (Don't use postfixes such as Service)