/// ReturnTy helps deciding
/// if the loop should continue or break.
#[derive(Debug)]
pub enum ReturnTy {
    Break,
    Continue,
    Expr(ExprTy),
}

#[derive(Debug, Default, derive_setters::Setters)]
#[setters(strip_option, into)]
pub struct ExprTy {
    pub stdout: String,
    pub stderr: Option<String>,
}

impl ExprTy {
    pub fn is_err(&self) -> bool {
        self.stderr.is_some()
    }
    pub fn as_string(&self) -> &str {
        &self.stdout
    }
}
