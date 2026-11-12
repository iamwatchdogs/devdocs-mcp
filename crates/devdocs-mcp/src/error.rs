//! The MCP tool-result construction and the error mapping
//! (docs/implementation/08 §MCP adapter).
//!
//! Every recoverable failure returns the CLI error envelope JSON as a
//! tool-level error result, so the caller sees the same single-line JSON
//! the CLI prints on stderr. Malformed arguments go through the same
//! envelope: [`reshape_argument_error`] replaces the router's raw
//! deserialization text with the `USAGE` envelope. Only protocol-level
//! failures — an unknown tool, this adapter's own plumbing — surface as
//! rmcp [`ErrorData`].

use devdocs_model::error::{Error, ErrorEnvelope};
use devdocs_retrieve::wiring;
use rmcp::model::{CallToolResult, ContentBlock, ErrorData};
use serde_json::Value;

/// The rmcp router turns a tool-argument deserialization failure into a
/// tool-level error result whose text starts with this prefix. The
/// constant is private in rmcp 3.9.0 (`src/handler/server/router/tool.rs`
/// `TOOL_ARGUMENT_DESERIALIZATION_ERROR_PREFIX`) and duplicated here; if
/// an rmcp bump renames it, the schema test fails at the next run of
/// `just test` (`crates/devdocs-mcp/tests/schema.rs`).
const TOOL_ARGUMENT_DESERIALIZATION_ERROR_PREFIX: &str = "failed to deserialize parameters:";

/// Reshapes the router's deserialization-failure result into the error
/// envelope: the raw rmcp text moves into `detail` unchanged, so the
/// caller keeps it. The stable `USAGE` code replaces the free-form
/// message, so every recoverable MCP failure carries one shape
/// (docs/implementation/08 §MCP adapter).
pub(crate) fn reshape_argument_error(result: CallToolResult) -> CallToolResult {
    if result.is_error != Some(true) {
        return result;
    }
    let Some(text) = result.content.first().and_then(ContentBlock::as_text) else {
        return result;
    };
    if !text
        .text
        .starts_with(TOOL_ARGUMENT_DESERIALIZATION_ERROR_PREFIX)
    {
        return result;
    }
    let envelope = ErrorEnvelope {
        error: String::from("USAGE"),
        hint: Some(String::from("match the tool schema in tools/list")),
        detail: Some(text.text.clone()),
        miss_reason: None,
        key_path: None,
    };
    match envelope.to_json_string() {
        Ok(json) => CallToolResult::error(vec![ContentBlock::text(json)]),
        // defensive: serializing a field-only struct cannot fail; the
        // arm keeps the router result intact instead of a panic site.
        // Unlike `tool_error`, which escalates to a protocol error, the
        // router text is foreign input, so a reshape bug keeps the raw
        // text rather than a protocol error.
        Err(_) => result,
    }
}

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

#[cfg(test)]
mod tests {
    use super::internal;
    use rmcp::model::ErrorCode;

    // The protocol-level internal error carries the JSON-RPC -32603
    // code and the detail as the message; no tool data flows through it.
    #[test]
    fn internal_carries_the_json_rpc_internal_error_code() {
        let error = internal(String::from("the search task failed: boom"));
        assert_eq!(error.code, ErrorCode::INTERNAL_ERROR);
        assert_eq!(error.message, "the search task failed: boom");
        assert_eq!(error.data, None);
    }
}
