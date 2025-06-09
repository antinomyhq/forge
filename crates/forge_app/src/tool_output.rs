use forge_domain::ToolResult;

use crate::{
    FetchOutput, FsCreateOutput, FsRemoveOutput, PatchOutput, ReadOutput, SearchResult, ShellOutput,
};

#[derive(derive_more::From)]
pub enum ToolOutput {
    FsRead(ReadOutput),
    FsCreate(FsCreateOutput),
    FsRemove(FsRemoveOutput),
    FsSearch(Option<SearchResult>),
    FsPatch(PatchOutput),
    // FIXME: conflicting impl with AttemptCompletion
    // need better output type
    // FsUndo(String),
    NetFetch(FetchOutput),
    Shell(ShellOutput),
    AttemptCompletion(String),
    FollowUp(Option<String>),
}

impl From<ToolOutput> for ToolResult {
    fn from(value: ToolOutput) -> Self {
        match value {
            ToolOutput::FsRead(_) => unimplemented!(),
            ToolOutput::FsCreate(_) => unimplemented!(),
            ToolOutput::FsRemove(_) => unimplemented!(),
            ToolOutput::FsSearch(_) => unimplemented!(),
            ToolOutput::FsPatch(_) => unimplemented!(),
            // ToolOutput::FsUndo(_) => unimplemented!(),
            ToolOutput::NetFetch(_) => unimplemented!(),
            ToolOutput::Shell(_) => unimplemented!(),
            ToolOutput::AttemptCompletion(_) => unimplemented!(),
            ToolOutput::FollowUp(_) => unimplemented!(),
        }
    }
}
