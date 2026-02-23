pub use include_dir::{Dir, DirEntry, File};
pub use include_dir::include_dir;

use handlebars::Handlebars;

/// Returns an iterator over all files embedded in `dir`, recursively
/// descending into subdirectories.
pub fn files(dir: &'static Dir<'static>) -> impl Iterator<Item = &'static File<'static>> {
    dir.entries().iter().flat_map(walk_entry)
}

fn walk_entry(entry: &'static DirEntry<'static>) -> Box<dyn Iterator<Item = &'static File<'static>>> {
    match entry {
        DirEntry::File(f) => Box::new(std::iter::once(f)),
        DirEntry::Dir(d) => Box::new(d.entries().iter().flat_map(walk_entry)),
    }
}

/// Registers all files in `dir` (recursively) as Handlebars templates.
///
/// Template names match the relative file path as returned by
/// [`File::path`] (e.g. `forge-system-prompt.md`). Panics if any file
/// contains invalid UTF-8 or if template parsing fails.
pub fn register_templates(hb: &mut Handlebars<'_>, dir: &'static Dir<'static>) {
    for file in files(dir) {
        let name = file.path().to_string_lossy();
        let content = file
            .contents_utf8()
            .unwrap_or_else(|| panic!("embedded template '{}' is not valid UTF-8", name));
        hb.register_template_string(&name, content)
            .unwrap_or_else(|e| panic!("failed to register template '{}': {}", name, e));
    }
}
