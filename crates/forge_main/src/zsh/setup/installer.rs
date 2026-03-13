use std::future::Future;
use std::pin::Pin;

/// A unit of installation work.
#[async_trait::async_trait]
pub trait Installation: Send {
    async fn install(self) -> anyhow::Result<()>;
}

/// A no-op installation that always succeeds.
///
/// Useful as a placeholder when you need a `Group` that does nothing
/// (e.g., as the seed for a builder chain).
pub struct Noop;

#[async_trait::async_trait]
impl Installation for Noop {
    async fn install(self) -> anyhow::Result<()> {
        Ok(())
    }
}

/// Type alias for a type-erased, boxed installation closure.
type BoxedInstall =
    Box<dyn FnOnce() -> Pin<Box<dyn Future<Output = anyhow::Result<()>> + Send>> + Send>;

/// Type alias for the success callback.
type OnOk = Box<dyn FnOnce() -> anyhow::Result<()> + Send>;

/// Type alias for the failure callback.
type OnErr = Box<dyn FnOnce(anyhow::Error) -> anyhow::Result<()> + Send>;

/// A composable group of installation tasks that can be executed
/// sequentially, in parallel, conditionally, or with result callbacks.
///
/// ```ignore
/// Group::unit(install_a)
///     .notify_ok(|| println!("a done"))
///     .notify_err(|e| { eprintln!("a failed: {e}"); Err(e) })
///     .then(Group::unit(install_b))
///     .alongside(Group::unit(install_c))
/// ```
pub enum Group {
    /// A single type-erased installation.
    Unit(BoxedInstall),
    /// Run the left group first, then run the right group.
    Sequential(Box<Group>, Box<Group>),
    /// Run both groups concurrently.
    Parallel(Box<Group>, Box<Group>),
    /// Run the inner group only if the condition is true; otherwise no-op.
    When(bool, Box<Group>),
    /// Run the inner group, then dispatch to callbacks based on the result.
    Notify {
        inner: Box<Group>,
        on_ok: Option<OnOk>,
        on_err: Option<OnErr>,
    },
}

impl Group {
    /// Creates a `Group::Unit` from any `Installation` implementor.
    pub fn unit(installation: impl Installation + 'static) -> Self {
        Group::Unit(Box::new(|| Box::pin(installation.install())))
    }

    /// Creates a conditional group that only runs if `condition` is true.
    pub fn when(condition: bool, group: Group) -> Self {
        Group::When(condition, Box::new(group))
    }

    /// Appends another group to run after this one completes.
    pub fn then(self, next: Group) -> Self {
        Group::Sequential(Box::new(self), Box::new(next))
    }

    /// Appends another group to run concurrently with this one.
    pub fn alongside(self, other: Group) -> Self {
        Group::Parallel(Box::new(self), Box::new(other))
    }

    /// Attaches a success callback to this group.
    pub fn notify_ok(self, on_ok: impl FnOnce() -> anyhow::Result<()> + Send + 'static) -> Self {
        match self {
            Group::Notify { inner, on_ok: _, on_err } => {
                Group::Notify { inner, on_ok: Some(Box::new(on_ok)), on_err }
            }
            other => Group::Notify {
                inner: Box::new(other),
                on_ok: Some(Box::new(on_ok)),
                on_err: None,
            },
        }
    }

    /// Attaches a failure callback to this group.
    pub fn notify_err(
        self,
        on_err: impl FnOnce(anyhow::Error) -> anyhow::Result<()> + Send + 'static,
    ) -> Self {
        match self {
            Group::Notify { inner, on_ok, on_err: _ } => {
                Group::Notify { inner, on_ok, on_err: Some(Box::new(on_err)) }
            }
            other => Group::Notify {
                inner: Box::new(other),
                on_ok: None,
                on_err: Some(Box::new(on_err)),
            },
        }
    }
}

#[async_trait::async_trait]
impl Installation for Group {
    async fn install(self) -> anyhow::Result<()> {
        match self {
            Group::Unit(f) => f().await,
            Group::Sequential(left, right) => {
                left.install().await?;
                right.install().await
            }
            Group::Parallel(left, right) => {
                let (l, r) = tokio::join!(left.install(), right.install());
                l.and(r)
            }
            Group::When(condition, inner) => {
                if condition {
                    inner.install().await
                } else {
                    Ok(())
                }
            }
            Group::Notify { inner, on_ok, on_err } => match inner.install().await {
                Ok(()) => match on_ok {
                    Some(f) => f(),
                    None => Ok(()),
                },
                Err(e) => match on_err {
                    Some(f) => f(e),
                    None => Err(e),
                },
            },
        }
    }
}
