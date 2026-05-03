use std::sync::Arc;

use syntect::{dumps::from_uncompressed_data, parsing::SyntaxSet};

use crate::diagnostic::BarDiagnostic;

/// # Errors
/// Returns error if the syntax set cannot be deserialized.
pub fn init() -> Result<Arc<SyntaxSet>, BarDiagnostic> {
    Ok(Arc::new(from_uncompressed_data(include_bytes!(
        "./syntaxes.bin"
    ))?))
}
