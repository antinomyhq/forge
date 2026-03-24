use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Resolves short, disambiguated nicknames for a list of directory paths.
///
/// The algorithm starts with the last path component as the nickname for each
/// path. When two or more paths share the same nickname, each conflicting one
/// is extended by prepending the next parent component. This repeats until all
/// nicknames are unique or the full path is consumed.
///
/// # Arguments
/// * `paths` - Slice of directory paths to resolve nicknames for.
///
/// # Returns
/// A map from original path to its disambiguated nickname string.
pub fn resolve_nicknames(paths: &[PathBuf]) -> HashMap<PathBuf, String> {
    if paths.is_empty() {
        return HashMap::new();
    }

    // Collect the components of each path in reverse order (leaf first).
    let components: Vec<Vec<String>> = paths
        .iter()
        .map(|p| {
            p.components()
                .rev()
                .map(|c| c.as_os_str().to_string_lossy().into_owned())
                .collect()
        })
        .collect();

    // Start with 1 component (just the leaf) for every path.
    let mut depths: Vec<usize> = vec![1; paths.len()];

    loop {
        // Build current nicknames at each path's depth.
        let nicknames: Vec<String> = components
            .iter()
            .zip(depths.iter())
            .map(|(comps, &depth)| build_nickname(comps, depth))
            .collect();

        // Group indices by nickname to find collisions.
        let mut groups: HashMap<&str, Vec<usize>> = HashMap::new();
        for (i, nick) in nicknames.iter().enumerate() {
            groups.entry(nick.as_str()).or_default().push(i);
        }

        let mut any_extended = false;
        for indices in groups.values() {
            if indices.len() > 1 {
                // Collision: try to extend each conflicting path by one more
                // component, but only if it has more components available.
                for &i in indices {
                    if depths[i] < components[i].len() {
                        depths[i] += 1;
                        any_extended = true;
                    }
                }
            }
        }

        if !any_extended {
            // No further disambiguation possible or all unique.
            let mut result = HashMap::with_capacity(paths.len());
            for (i, path) in paths.iter().enumerate() {
                result
                    .entry(path.clone())
                    .or_insert_with(|| nicknames[i].clone());
            }
            return result;
        }
    }
}

/// Builds a nickname from reversed path components at the given depth.
fn build_nickname(reversed_components: &[String], depth: usize) -> String {
    let take = depth.min(reversed_components.len());
    let parts: Vec<&str> = reversed_components[..take]
        .iter()
        .rev()
        .map(|s| s.as_str())
        .collect();
    // Build using PathBuf to correctly handle the root separator.
    let mut path = PathBuf::new();
    for part in parts {
        path.push(part);
    }
    path.display().to_string()
}

/// Looks up the nickname for a specific path from a pre-computed nickname map.
///
/// Falls back to the full path display string if the path is not in the map.
pub fn nickname_for(path: &Path, nicknames: &HashMap<PathBuf, String>) -> String {
    nicknames
        .get(path)
        .cloned()
        .unwrap_or_else(|| path.display().to_string())
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn test_all_unique_last_components() {
        let paths = vec![
            PathBuf::from("/a/b/alpha"),
            PathBuf::from("/c/d/beta"),
            PathBuf::from("/e/f/gamma"),
        ];

        let actual = resolve_nicknames(&paths);

        assert_eq!(actual[&paths[0]], "alpha");
        assert_eq!(actual[&paths[1]], "beta");
        assert_eq!(actual[&paths[2]], "gamma");
    }

    #[test]
    fn test_two_paths_same_last_component() {
        let paths = vec![
            PathBuf::from("/a/b/server"),
            PathBuf::from("/c/d/server"),
        ];

        let actual = resolve_nicknames(&paths);

        assert_eq!(actual[&paths[0]], format!("b{}server", std::path::MAIN_SEPARATOR));
        assert_eq!(actual[&paths[1]], format!("d{}server", std::path::MAIN_SEPARATOR));
    }

    #[test]
    fn test_three_level_disambiguation() {
        let paths = vec![
            PathBuf::from("/x/a/c/server"),
            PathBuf::from("/y/b/c/server"),
        ];

        let actual = resolve_nicknames(&paths);

        // Both share "server" and "c/server", so need "a/c/server" vs
        // "b/c/server".
        let sep = std::path::MAIN_SEPARATOR;
        assert_eq!(actual[&paths[0]], format!("a{sep}c{sep}server"));
        assert_eq!(actual[&paths[1]], format!("b{sep}c{sep}server"));
    }

    #[test]
    fn test_single_path() {
        let paths = vec![PathBuf::from("/home/user/my-project")];

        let actual = resolve_nicknames(&paths);

        assert_eq!(actual[&paths[0]], "my-project");
    }

    #[test]
    fn test_identical_paths() {
        let paths = vec![
            PathBuf::from("/a/b/c"),
            PathBuf::from("/a/b/c"),
        ];

        let actual = resolve_nicknames(&paths);

        // Both map to the same key so only one entry in the HashMap.
        // The nickname should be the full path since they cannot be
        // disambiguated.
        let sep = std::path::MAIN_SEPARATOR;
        assert_eq!(actual[&paths[0]], format!("{sep}a{sep}b{sep}c"));
    }

    #[test]
    fn test_empty_input() {
        let paths: Vec<PathBuf> = vec![];

        let actual = resolve_nicknames(&paths);

        assert!(actual.is_empty());
    }

    #[test]
    fn test_mixed_unique_and_conflicting() {
        let paths = vec![
            PathBuf::from("/a/b/server"),
            PathBuf::from("/c/d/server"),
            PathBuf::from("/e/f/client"),
        ];

        let actual = resolve_nicknames(&paths);

        let sep = std::path::MAIN_SEPARATOR;
        assert_eq!(actual[&paths[0]], format!("b{sep}server"));
        assert_eq!(actual[&paths[1]], format!("d{sep}server"));
        assert_eq!(actual[&paths[2]], "client");
    }

    #[test]
    fn test_nickname_for_helper() {
        let paths = vec![
            PathBuf::from("/a/b/server"),
            PathBuf::from("/c/d/client"),
        ];
        let nicknames = resolve_nicknames(&paths);

        let actual = nickname_for(&PathBuf::from("/a/b/server"), &nicknames);
        assert_eq!(actual, "server");

        let missing = nickname_for(&PathBuf::from("/unknown/path"), &nicknames);
        assert_eq!(missing, "/unknown/path");
    }
}
