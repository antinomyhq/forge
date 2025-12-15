use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use forge_app::EnvironmentInfra;
use forge_domain::Environment;

#[derive(Clone)]
pub struct ForgeEnvironmentInfra {
    restricted: bool,
    cwd: PathBuf,
}

impl ForgeEnvironmentInfra {
    /// Creates a new EnvironmentFactory with specified working directory
    ///
    /// # Arguments
    /// * `restricted` - If true, use restricted shell mode (rbash) If false,
    ///   use unrestricted shell mode (sh/bash)
    /// * `cwd` - Required working directory path
    pub fn new(restricted: bool, cwd: PathBuf) -> Self {
        Self::dot_env(&cwd);
        Self { restricted, cwd }
    }

    /// Get path to appropriate shell based on platform and mode
    fn get_shell_path(&self) -> String {
        if cfg!(target_os = "windows") {
            std::env::var("COMSPEC").unwrap_or("cmd.exe".to_string())
        } else if self.restricted {
            // Default to rbash in restricted mode
            "/bin/rbash".to_string()
        } else {
            // Use user's preferred shell or fallback to sh
            std::env::var("SHELL").unwrap_or("/bin/sh".to_string())
        }
    }

    fn get(&self) -> Environment {
        // Load environment configuration using the config crate
        let mut env = Environment::from_env().expect("Failed to load environment configuration");
        
        // Override fields specific to this infrastructure instance
        env.cwd = self.cwd.clone();
        env.shell = self.get_shell_path();
        
        env
    }

    /// Load all `.env` files with priority to lower (closer) files.
    fn dot_env(cwd: &Path) -> Option<()> {
        let mut paths = vec![];
        let mut current = PathBuf::new();

        for component in cwd.components() {
            current.push(component);
            paths.push(current.clone());
        }

        paths.reverse();

        for path in paths {
            let env_file = path.join(".env");
            if env_file.is_file() {
                dotenvy::from_path(&env_file).ok();
            }
        }

        Some(())
    }
}

impl EnvironmentInfra for ForgeEnvironmentInfra {
    fn get_environment(&self) -> Environment {
        self.get()
    }

    fn get_env_var(&self, key: &str) -> Option<String> {
        std::env::var(key).ok()
    }

    fn get_env_vars(&self) -> BTreeMap<String, String> {
        // TODO: Maybe cache it?
        std::env::vars().collect()
    }
}



#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::{env, fs};

    use serial_test::serial;
    use tempfile::{TempDir, tempdir};

    use super::*;

    fn setup_envs(structure: Vec<(&str, &str)>) -> (TempDir, PathBuf) {
        let root = tempdir().unwrap();
        let root_path = root.path().to_path_buf();

        for (rel_path, content) in &structure {
            let dir = root_path.join(rel_path);
            fs::create_dir_all(&dir).unwrap();
            fs::write(dir.join(".env"), content).unwrap();
        }

        let deepest_path = root_path.join(structure[0].0);
        // We MUST return root path, because dropping it will remove temp dir
        (root, deepest_path)
    }

    #[test]
    #[serial]
    fn test_dot_env_loading() {
        // Test single env file
        let (_root, cwd) = setup_envs(vec![("", "TEST_KEY1=VALUE1")]);
        ForgeEnvironmentInfra::dot_env(&cwd);
        assert_eq!(env::var("TEST_KEY1").unwrap(), "VALUE1");

        // Test nested env files with override (closer files win)
        let (_root, cwd) = setup_envs(vec![("a/b", "TEST_KEY2=SUB"), ("a", "TEST_KEY2=ROOT")]);
        ForgeEnvironmentInfra::dot_env(&cwd);
        assert_eq!(env::var("TEST_KEY2").unwrap(), "SUB");

        // Test multiple keys from different levels
        let (_root, cwd) = setup_envs(vec![
            ("a/b", "SUB_KEY3=SUB_VAL"),
            ("a", "ROOT_KEY3=ROOT_VAL"),
        ]);
        ForgeEnvironmentInfra::dot_env(&cwd);
        assert_eq!(env::var("ROOT_KEY3").unwrap(), "ROOT_VAL");
        assert_eq!(env::var("SUB_KEY3").unwrap(), "SUB_VAL");

        // Test standard env precedence (std env wins over .env files)
        let (_root, cwd) = setup_envs(vec![("a/b", "TEST_KEY4=SUB_VAL")]);
        unsafe {
            env::set_var("TEST_KEY4", "STD_ENV_VAL");
        }
        ForgeEnvironmentInfra::dot_env(&cwd);
        assert_eq!(env::var("TEST_KEY4").unwrap(), "STD_ENV_VAL");
    }



    #[test]
    #[serial]
    fn test_multiline_env_vars() {
        let content = r#"MULTI_LINE='line1
line2
line3'
SIMPLE=value"#;

        let (_root, cwd) = setup_envs(vec![("", content)]);
        ForgeEnvironmentInfra::dot_env(&cwd);

        // Verify multiline variable
        let multi = env::var("MULTI_LINE").expect("MULTI_LINE should be set");
        assert_eq!(multi, "line1\nline2\nline3");

        // Verify simple var
        assert_eq!(env::var("SIMPLE").unwrap(), "value");

        unsafe {
            env::remove_var("MULTI_LINE");
            env::remove_var("SIMPLE");
        }
    }
}
