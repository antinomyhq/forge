use std::future::Future;
use std::pin::Pin;

/// A unit of installation work.
#[async_trait::async_trait]
pub trait Installation: Send {
    async fn install(self) -> anyhow::Result<()>;
}

/// A no-op installation that always succeeds.
///
/// Useful as a placeholder in `Task` when you only need the callbacks
/// (e.g., to append a success message after a `Group` completes).
pub struct Noop;

#[async_trait::async_trait]
impl Installation for Noop {
    async fn install(self) -> anyhow::Result<()> {
        Ok(())
    }
}

/// A task that wraps an `Installation` with success and failure callbacks.
///
/// The `on_ok` callback is invoked when the installation succeeds, and
/// `on_err` is invoked with the error when it fails. Both callbacks
/// return `anyhow::Result<()>` so the caller can decide whether to
/// propagate or swallow the error.
pub struct Task<I, Ok, Fail> {
    installation: I,
    on_ok: Ok,
    on_err: Fail,
}

impl<I, Ok, Fail> Task<I, Ok, Fail>
where
    I: Installation + 'static,
    Ok: FnOnce() -> anyhow::Result<()> + Send + 'static,
    Fail: FnOnce(anyhow::Error) -> anyhow::Result<()> + Send + 'static,
{
    /// Creates a new `Task` wrapping the given installation with callbacks.
    pub fn new(installation: I, on_ok: Ok, on_err: Fail) -> Self {
        Self { installation, on_ok, on_err }
    }

    /// Runs the installation, then dispatches to the appropriate callback.
    pub async fn execute(self) -> anyhow::Result<()> {
        match self.installation.install().await {
            Result::Ok(()) => (self.on_ok)(),
            Err(e) => (self.on_err)(e),
        }
    }

    /// Converts this task into a type-erased `Group::Unit`.
    pub fn into_group(self) -> Group {
        Group::task(self)
    }
}

/// Type alias for the boxed closure stored inside `Group::Unit`.
type BoxedTask =
    Box<dyn FnOnce() -> Pin<Box<dyn Future<Output = anyhow::Result<()>> + Send>> + Send>;

/// A composable group of installation tasks that can be executed
/// sequentially or in parallel.
///
/// The structure is left-associative: `Sequential` and `Parallel` always
/// chain an existing `Group` (left) with a single new `Task` (right).
/// This naturally maps to a builder pattern:
///
/// ```ignore
/// task_a.into_group()
///     .then(task_b)       // Sequential(Unit(a), b)
///     .alongside(task_c)  // Parallel(Sequential(Unit(a), b), c)
/// ```
pub enum Group {
    /// A single task (type-erased `Task` with its callbacks).
    Unit(BoxedTask),
    /// Run the group first, then run the task.
    Sequential(Box<Group>, BoxedTask),
    /// Run the group and the task concurrently.
    Parallel(Box<Group>, BoxedTask),
}

impl Group {
    /// Creates a `Group::Unit` from a `Task` with callbacks.
    pub fn task<I, Ok, Fail>(task: Task<I, Ok, Fail>) -> Self
    where
        I: Installation + 'static,
        Ok: FnOnce() -> anyhow::Result<()> + Send + 'static,
        Fail: FnOnce(anyhow::Error) -> anyhow::Result<()> + Send + 'static,
    {
        Group::Unit(Box::new(|| Box::pin(task.execute())))
    }

    /// Appends a task to run after this group completes.
    pub fn then<I, Ok, Fail>(self, task: Task<I, Ok, Fail>) -> Self
    where
        I: Installation + 'static,
        Ok: FnOnce() -> anyhow::Result<()> + Send + 'static,
        Fail: FnOnce(anyhow::Error) -> anyhow::Result<()> + Send + 'static,
    {
        Group::Sequential(Box::new(self), Self::boxed_task(task))
    }

    /// Appends a task to run concurrently with this group.
    pub fn alongside<I, Ok, Fail>(self, task: Task<I, Ok, Fail>) -> Self
    where
        I: Installation + 'static,
        Ok: FnOnce() -> anyhow::Result<()> + Send + 'static,
        Fail: FnOnce(anyhow::Error) -> anyhow::Result<()> + Send + 'static,
    {
        Group::Parallel(Box::new(self), Self::boxed_task(task))
    }

    /// Executes the group, returning a pinned future.
    pub fn execute(self) -> Pin<Box<dyn Future<Output = anyhow::Result<()>> + Send>> {
        Box::pin(async move {
            match self {
                Group::Unit(task_fn) => task_fn().await,
                Group::Sequential(group, task_fn) => {
                    group.execute().await?;
                    task_fn().await
                }
                Group::Parallel(group, task_fn) => {
                    let (l, r) = tokio::join!(group.execute(), task_fn());
                    l.and(r)
                }
            }
        })
    }

    /// Type-erases a `Task` into a `BoxedTask` closure.
    fn boxed_task<I, Ok, Fail>(task: Task<I, Ok, Fail>) -> BoxedTask
    where
        I: Installation + 'static,
        Ok: FnOnce() -> anyhow::Result<()> + Send + 'static,
        Fail: FnOnce(anyhow::Error) -> anyhow::Result<()> + Send + 'static,
    {
        Box::new(|| Box::pin(task.execute()))
    }
}

impl<I, Ok, Fail> From<Task<I, Ok, Fail>> for Group
where
    I: Installation + 'static,
    Ok: FnOnce() -> anyhow::Result<()> + Send + 'static,
    Fail: FnOnce(anyhow::Error) -> anyhow::Result<()> + Send + 'static,
{
    fn from(task: Task<I, Ok, Fail>) -> Self {
        Group::task(task)
    }
}

#[derive(Default)]
pub struct Installer {
    groups: Vec<Group>,
}

impl Installer {
    pub fn add(mut self, group: Group) -> Self {
        self.groups.push(group);
        self
    }

    pub async fn execute(self) -> anyhow::Result<()> {
        for group in self.groups {
            group.execute().await?;
        }
        Ok(())
    }
}
