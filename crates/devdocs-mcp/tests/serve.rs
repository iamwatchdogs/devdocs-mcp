//! The `serve` startup path (docs/implementation/08 §MCP adapter): a
//! store that cannot open fails the server before any transport work
//! starts, so a bad config surfaces as one clean upstream failure.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use devdocs_mcp::serve;
use devdocs_model::error::{Error, UpstreamCode};

/// A data root that cannot be created — the path crosses a regular
/// file — fails startup with the upstream error the exit-code map
/// serves exit 3 for; the rmcp transport never starts.
#[test]
fn serve_fails_when_the_data_directory_cannot_be_created() {
    let dir = tempfile::tempdir().unwrap();
    let blocker = dir.path().join("blocker");
    std::fs::write(&blocker, b"not a directory").unwrap();
    let mut config = devdocs_testkit::base_config();
    config.index.path = blocker.join("docs.db");
    let result = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(serve(&config));
    match result {
        Err(Error::Upstream { code, detail }) => {
            assert_eq!(code, UpstreamCode::UpstreamError);
            assert!(
                detail.contains("cannot create the data directory"),
                "detail: {detail}"
            );
        }
        other => panic!("expected an upstream startup failure, got: {other:?}"),
    }
}
