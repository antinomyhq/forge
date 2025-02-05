mod approve;
mod ask;
mod fetch;
mod fs;
mod outline;
mod patch;
mod select;
mod shell;
mod syn;
mod think;
mod utils;

use fetch::Fetch;
use forge_domain::Tool;
use fs::*;
use outline::Outline;
use patch::*;
use shell::Shell;
use think::Think;

pub fn tools() -> Vec<Tool> {
    vec![
        // Approve.into(),
        FSRead.into(),
        FSWrite.into(),
        FSList::default().into(),
        FSSearch.into(),
        FSFileInfo.into(),
        // TODO: once ApplyPatchJson is stable we can delete ApplyPatch
        ApplyPatch.into(),
        // ApplyPatchJson.into(),
        Outline.into(),
        // SelectTool.into(),
        Shell::default().into(),
        Think::default().into(),
        Fetch::default().into(),
    ]
}

pub fn is_default<T: Default + PartialEq>(value: &T) -> bool {
    *value == T::default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tool_description_length() {
        const MAX_DESCRIPTION_LENGTH: usize = 1024;

        println!("\nTool description lengths:");

        let mut any_exceeded = false;
        for tool in tools() {
            let desc_len = tool.definition.description.len();
            println!(
                "{:?}: {} chars {}",
                tool.definition.name,
                desc_len,
                if desc_len > MAX_DESCRIPTION_LENGTH {
                    "(!)"
                } else {
                    ""
                }
            );

            if desc_len > MAX_DESCRIPTION_LENGTH {
                any_exceeded = true;
            }
        }

        assert!(
            !any_exceeded,
            "One or more tools exceed the maximum description length of {}",
            MAX_DESCRIPTION_LENGTH
        );
    }
}
