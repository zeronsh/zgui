//! `zgx inspect mcp`: devtools as MCP tools, the handshake and the tools
//! an agent sees (asking an app is `zgx-run`'s tests).
use std::io::Write;
use std::process::{Command, Stdio};

#[test]
fn the_mcp_server_says_what_it_offers() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_zgx"))
        .args(["inspect", "mcp"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let requests = [
        r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"test","version":"1"}}}"#,
        r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#,
        r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#,
        r#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"zgx_nope","arguments":{}}}"#,
    ];
    {
        let mut stdin = child.stdin.take().unwrap();
        for request in requests {
            writeln!(stdin, "{request}").unwrap();
        }
    }
    let out = child.wait_with_output().unwrap();
    let lines: Vec<&str> = std::str::from_utf8(&out.stdout).unwrap().lines().collect();
    // One answer a request with an id; none for the notification.
    assert_eq!(lines.len(), 3, "{lines:?}");
    assert!(
        lines[0].contains(r#""id":1"#) && lines[0].contains(r#""tools":{"listChanged":false}"#),
        "{}",
        lines[0]
    );
    assert!(lines[0].contains(r#""protocolVersion":"2025-06-18""#));
    for tool in [
        "zgx_tree",
        "zgx_click",
        "zgx_type",
        "zgx_key",
        "zgx_state",
        "zgx_frames",
        "zgx_wait",
        "zgx_snapshot",
    ] {
        assert!(
            lines[1].contains(&format!(r#""name":"{tool}""#)),
            "{tool} in {}",
            lines[1]
        );
    }
    assert!(
        lines[1].contains(r#""required":["target"]"#),
        "targets required where they are"
    );
    assert!(
        lines[2].contains(r#""isError":true"#) && lines[2].contains("no tool zgx_nope"),
        "{}",
        lines[2]
    );
}
