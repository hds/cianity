use std::path::Path;

use ciane::{
    ast::{AstNode, Root},
    formatter, parse,
};

/// Read, parse, format, and optionally write back a `ciane` source file.
///
/// In check mode, returns `Err` if the file content differs from the
/// formatted output.  In write mode, overwrites the file in place.
///
/// # Errors
///
/// Returns `Err` if the file cannot be read or written, if the source has
/// parse errors, or if the formatter cannot handle a construct in the file.
pub fn run(path: &Path, check_only: bool) -> anyhow::Result<()> {
    let source =
        std::fs::read_to_string(path).map_err(|e| anyhow::anyhow!("cannot read file: {e}"))?;

    let result = parse(&source);
    if !result.errors().is_empty() {
        anyhow::bail!("cannot format {}: file has parse errors", path.display());
    }

    let root = Root::cast(result.syntax())
        .ok_or_else(|| anyhow::anyhow!("internal error: parse produced no Root node"))?;

    let formatted =
        formatter::format(&root).map_err(|e| anyhow::anyhow!("{}: {e}", path.display()))?;

    if check_only {
        if source == formatted {
            Ok(())
        } else {
            anyhow::bail!("{} is not formatted", path.display())
        }
    } else {
        std::fs::write(path, formatted).map_err(|e| anyhow::anyhow!("cannot write file: {e}"))?;
        Ok(())
    }
}
