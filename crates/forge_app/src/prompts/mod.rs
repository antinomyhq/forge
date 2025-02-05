use handlebars::Handlebars;
use serde::Serialize;

pub struct Prompt {
    template: String,
}

impl Prompt {
    pub fn new(template: impl ToString) -> Self {
        Self { template: template.to_string() }
    }

    pub fn render(&self, data: &impl Serialize) -> anyhow::Result<String> {
        let mut hb = Handlebars::new();
        hb.set_strict_mode(true);
        hb.register_escape_fn(|str| str.to_string());
        hb.register_partial("tool_use_example", include_str!("./tool_use_example.md"))?;
        hb.register_partial("tool_use", include_str!("./tool_use.md"))?;

        Ok(hb.render_template(&self.template, data)?)
    }
}
