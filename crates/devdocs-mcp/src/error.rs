//! The MCP tool-result construction and the error mapping
//! (docs/implementation/08 §MCP adapter).
//!
//! Every recoverable failure returns the CLI error envelope JSON as a
//! tool-level error result, so the caller sees the same single-line JSON
//! the CLI prints on stderr. Only protocol-level failures — an unknown
//! tool, malformed arguments, this adapter's own plumbing — surface as
//! rmcp [`ErrorData`].

use devdocs_model::error::Error;
use devdocs_retrieve::wiring;
use rmcp::model::{CallToolResult, ContentBlock, ErrorData};
use serde_json::Value;

/// Maps one retrieval failure onto the tool-level error result carrying
/// the CLI error envelope JSON.
pub(crate) fn from_retrieve(
    error: devdocs_retrieve::RetrieveError,
) -> Result<CallToolResult, ErrorData> {
    tool_error(&wiring::map_retrieve_error(error))
}

/// Maps one model error onto the tool-level error result carrying the
/// CLI error envelope JSON (`{"error", "hint", "detail", ...}`).
pub(crate) fn tool_error(error: &Error) -> Result<CallToolResult, ErrorData> {
    let json = error
        .to_envelope()
        .to_json_string()
        .map_err(|e| internal(format!("cannot serialize the error envelope: {e}")))?;
    Ok(CallToolResult::error(vec![ContentBlock::text(json)]))
}

/// Builds a success result whose text content is `json` verbatim and
/// whose structured content is the same JSON parsed.
///
/// rmcp 3.8.0 marks `CallToolResult` `#[non_exhaustive]` and offers no
/// constructor that sets an exact text block and the structured content
/// together: `CallToolResult::structured` serializes the value again,
/// which reorders object keys and would break byte identity with the CLI
/// envelope. This helper round-trips the serde form to attach both
/// fields.
///
/// # Errors
/// [`ErrorData`] when `json` does not parse, the result does not
/// serialize, or the round trip does not decode.
pub(crate) fn success_result(json: String) -> Result<CallToolResult, ErrorData> {
    let structured: Value = serde_json::from_str(&json)
        .map_err(|e| internal(format!("cannot parse the result JSON: {e}")))?;
    let mut raw = serde_json::to_value(CallToolResult::success(vec![ContentBlock::text(json)]))
        .map_err(|e| internal(format!("cannot serialize the call result: {e}")))?;
    let Some(object) = raw.as_object_mut() else {
        // defensive: a struct always serializes to a JSON object; the arm
        // keeps the mapping total without a panic site.
        return Err(internal(String::from(
            "the call result did not serialize to an object",
        )));
    };
    object.insert(String::from("structuredContent"), structured);
    serde_json::from_value(raw).map_err(|e| internal(format!("cannot decode the call result: {e}")))
}

/// The protocol-level internal error the adapter reports when its own
/// plumbing fails; no tool data flows through it.
pub(crate) fn internal(detail: String) -> ErrorData {
    ErrorData::internal_error(detail, None)
}
