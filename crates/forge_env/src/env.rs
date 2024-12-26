use handlebars::Handlebars;
use serde::Serialize;

use crate::Result;

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
            current_working_dir: format!("{}", std::env::current_dir()?.display()),
            default_shell: if cfg!(windows) {
                std::env::var("COMSPEC").expect("Failed to get default shell in windows.")
            } else {
                std::env::var("SHELL").expect("Failed to get default shell.")
            },
            home_directory: dirs::home_dir()
                .expect("Failed to get home directory")
                .display()
                .to_string(),
        };

        let mut hb = Handlebars::new();
        hb.set_strict_mode(true);
        Ok(hb.render_template(template, &env)?)
    }
}
