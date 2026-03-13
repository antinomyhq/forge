#[async_trait::async_trait]
pub trait Installation: Send + Sync {
    async fn install(&self) -> anyhow::Result<()>;
}

pub enum Group {
    Unit(Box<dyn Installation>),
    Sequential(Box<Group>, Box<Group>),
    Parallel(Box<Group>, Box<Group>),
}

impl Group {
    pub fn unit<V: Installation + 'static>(v: V) -> Self {
        Group::Unit(Box::new(v))
    }

    pub fn then(self, rhs: impl Into<Group>) -> Self {
        Group::Sequential(Box::new(self), Box::new(rhs.into()))
    }

    pub fn alongside(self, rhs: impl Into<Group>) -> Self {
        Group::Parallel(Box::new(self), Box::new(rhs.into()))
    }
    pub fn execute(
        self,
    ) -> std::pin::Pin<Box<dyn Future<Output = anyhow::Result<()>> + Send>> {
        Box::pin(async move {
            match self {
                Group::Unit(installation) => installation.install().await,
                Group::Sequential(left, right) => {
                    left.execute().await?;
                    right.execute().await
                }
                Group::Parallel(left, right) => {
                    let (l, r) = tokio::join!(left.execute(), right.execute());
                    l.and(r)
                }
            }
        })
    }
}

impl<T: Installation + 'static> From<T> for Group {
    fn from(value: T) -> Self {
        Group::unit(value)
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
