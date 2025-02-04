use crate::{Context, Variables};

pub struct ContextExtension;

impl ContextExtension {
    pub fn insert_summary(&self, _context: Context, _variables: &Variables) -> Context {
        todo!()
    }

    pub fn enhance_user_message(&self, _context: Context, _variables: &Variables) -> Context {
        todo!()
    }
}
