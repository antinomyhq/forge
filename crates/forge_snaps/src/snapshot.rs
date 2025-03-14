use std::fmt::{Display, Formatter};
use std::hash::Hasher;
use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use forge_fs::ForgeFS;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A newtype for snapshot IDs, internally using UUID
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct SnapshotId(Uuid);

impl SnapshotId {
    /// Create a new random SnapshotId
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    /// Parse a SnapshotId from a string
    pub fn parse(s: &str) -> Option<Self> {
        Uuid::parse_str(s).ok().map(Self)
    }

    /// Get the underlying UUID
    pub fn uuid(&self) -> &Uuid {
        &self.0
    }
}

impl Default for SnapshotId {
    fn default() -> Self {
        Self::new()
    }
}

impl Display for SnapshotId {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<Uuid> for SnapshotId {
    fn from(uuid: Uuid) -> Self {
        Self(uuid)
    }
}

/// Represents information about a file snapshot
///
/// Contains details about when the snapshot was created,
/// the original file path, the snapshot location, and file size.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Snapshot {
    /// Unique ID for the file
    pub id: SnapshotId,

    /// Unix timestamp when the snapshot was created
    pub timestamp: Duration,

    /// Original file path that is being processed
    pub path: String,
}

impl Snapshot {
    pub async fn create(path: PathBuf) -> anyhow::Result<Self> {
        let path = path.canonicalize()?;
        let timestamp = SystemTime::now().duration_since(UNIX_EPOCH)?;

        Ok(Self {
            id: SnapshotId::new(),
            timestamp,
            path: path.display().to_string(),
        })
    }

    /// Create a hash of a file path for storage
    fn path_hash(&self) -> String {
        let mut hasher = fnv_rs::Fnv64::default();
        hasher.write(self.path.as_bytes());
        format!("{:x}", hasher.finish())
    }

    /// Create a snapshot filename from a path and timestamp
    pub fn snapshot_path(&self, cwd: Option<PathBuf>) -> PathBuf {
        // Convert Duration to SystemTime then to a formatted string
        let datetime = UNIX_EPOCH + self.timestamp;
        // Format: YYYY-MM-DD_HH-MM-SS-mmm (including milliseconds)
        let formatted_time = chrono::DateTime::<chrono::Utc>::from(datetime)
            .format("%Y-%m-%d_%H-%M-%S-%3f")
            .to_string();

        let filename = format!("{}.snap", formatted_time);
        let path = PathBuf::from(self.path_hash()).join(PathBuf::from(filename));
        if let Some(cwd) = cwd {
            cwd.join(path)
        } else {
            path
        }
    }

    pub async fn save(&self, path: Option<PathBuf>) -> anyhow::Result<()> {
        let content = ForgeFS::read(&self.path).await?;
        let path = self.snapshot_path(path);
        ForgeFS::write(path, content).await?;
        Ok(())
    }
}
