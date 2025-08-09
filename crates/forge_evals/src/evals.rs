use crate::tests::{Test1, Test2};

pub struct Score(f64);

pub trait Eval {
    // fn from_dump(dump_path: &str) -> Self;
    // fn from_call_dump(call_dump_path: &str) -> Self;
    fn eval(&self) -> Score;

    // fn from_context(context: &forge_domain::Context) -> Self;
    // Implement functions like this to directly create from in between code in
    // certain modes instead of first writing to a file
}

impl Eval for Test1 {
    fn eval(&self) -> Score {
        if self.no_incorrect_forge_tag_use() {
            Score(1.0)
        } else {
            Score(0.0)
        }
    }
}

impl Eval for Test2 {
    fn eval(&self) -> Score {
        if self.has_tool_completion_call() {
            Score(1.0)
        } else {
            Score(0.0)
        }
    }
}
