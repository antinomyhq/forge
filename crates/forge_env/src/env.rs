use handlebars::Handlebars;
use serde::Serialize;

use crate::{Error, Platform, Result};

#[derive(Serialize)]
struct EnvironmentValue {
    operating_system: String,
    current_working_dir: String,
    default_shell: String,
    home_directory: String,
}

pub struct Environment;

impl Environment {
    pub fn render(template: &str) -> Result<String> {
        let env = EnvironmentValue {
            operating_system: std::env::consts::OS.to_string(),
            current_working_dir: std::env::current_dir()?.display().to_string(),
            default_shell: if cfg!(windows) {
                std::env::var("COMSPEC").or(Err(Error::IndeterminateShell(Platform::Windows)))?
            } else {
                std::env::var("SHELL").or(Err(Error::IndeterminateShell(Platform::UnixLike)))?
            },
            home_directory: dirs::home_dir()
                .ok_or(Error::IndeterminateHomeDir)?
                .display()
                .to_string(),
        };

        let mut hb = Handlebars::new();
        hb.set_strict_mode(true);
        Ok(hb.render_template(template, &env)?)
    }
}
