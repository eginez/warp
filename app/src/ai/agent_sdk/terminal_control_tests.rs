use std::io::Cursor;

use serde_json::json;
use warp_cli::{
    agent::OutputFormat,
    terminal_control::{
        PaneTargetArgs, SendKeyArgs, SendKeyToPaneArgs, SendTextArgs, SendTextToPaneArgs,
        TerminalControlCommand,
    },
};

use crate::app_services::terminal_control_service::{
    PaneSummary, TabSummary, TerminalControlError, TerminalControlRequest, TerminalControlResponse,
    TerminalControlTarget, TerminalPaneSnapshot,
};

use super::{
    TerminalControlListPaneRow, TerminalControlListTabRow, command_to_request, format_error,
    write_response_to,
};

#[test]
fn terminal_control_send_builds_active_pane_request() {
    assert_eq!(
        command_to_request(TerminalControlCommand::Send(SendTextArgs {
            text: "ls\n".to_string(),
        }))
        .unwrap(),
        TerminalControlRequest::SendText {
            target: TerminalControlTarget::ActivePane,
            text: "ls\n".to_string(),
        }
    );
}

#[test]
fn terminal_control_create_tab_builds_request() {
    assert_eq!(
        command_to_request(TerminalControlCommand::CreateTab).unwrap(),
        TerminalControlRequest::CreateTab
    );
}

#[test]
fn terminal_control_send_pane_builds_explicit_target_request() {
    assert_eq!(
        command_to_request(TerminalControlCommand::SendPane(SendTextToPaneArgs {
            pane_id: "pane-123".to_string(),
            text: "pwd\n".to_string(),
        }))
        .unwrap(),
        TerminalControlRequest::SendText {
            target: TerminalControlTarget::PaneId("pane-123".to_string()),
            text: "pwd\n".to_string(),
        }
    );
}

#[test]
fn command_to_request_maps_other_terminal_control_commands() {
    assert_eq!(
        command_to_request(TerminalControlCommand::ListTabs).unwrap(),
        TerminalControlRequest::ListTabs
    );
    assert_eq!(
        command_to_request(TerminalControlCommand::ListPanes).unwrap(),
        TerminalControlRequest::ListPanes
    );
    assert_eq!(
        command_to_request(TerminalControlCommand::CurrentPane).unwrap(),
        TerminalControlRequest::CurrentPane
    );
    assert_eq!(
        command_to_request(TerminalControlCommand::FocusPane(PaneTargetArgs {
            pane_id: "pane-456".to_string(),
        }))
        .unwrap(),
        TerminalControlRequest::FocusPane {
            target: TerminalControlTarget::PaneId("pane-456".to_string()),
        }
    );
    assert_eq!(
        command_to_request(TerminalControlCommand::SendKey(SendKeyArgs {
            key: "enter".to_string(),
        }))
        .unwrap(),
        TerminalControlRequest::SendKey {
            target: TerminalControlTarget::ActivePane,
            key: "enter".to_string(),
        }
    );
    assert_eq!(
        command_to_request(TerminalControlCommand::SendKeyPane(SendKeyToPaneArgs {
            pane_id: "pane-789".to_string(),
            key: "tab".to_string(),
        }))
        .unwrap(),
        TerminalControlRequest::SendKey {
            target: TerminalControlTarget::PaneId("pane-789".to_string()),
            key: "tab".to_string(),
        }
    );
}

#[test]
fn terminal_control_read_builds_active_pane_request() {
    assert_eq!(
        command_to_request(TerminalControlCommand::Read).unwrap(),
        TerminalControlRequest::ReadPane {
            target: TerminalControlTarget::ActivePane,
        }
    );
}

#[test]
fn terminal_control_read_pane_builds_explicit_target_request() {
    assert_eq!(
        command_to_request(TerminalControlCommand::ReadPane(PaneTargetArgs {
            pane_id: "pane-123".to_string(),
        }))
        .unwrap(),
        TerminalControlRequest::ReadPane {
            target: TerminalControlTarget::PaneId("pane-123".to_string()),
        }
    );
}

#[test]
fn terminal_control_read_pretty_output_includes_metadata_and_raw_content() {
    let mut output = Cursor::new(Vec::new());

    write_response_to(
        &mut output,
        &TerminalControlResponse::ReadPane(sample_snapshot()),
        OutputFormat::Pretty,
    )
    .unwrap();

    assert_eq!(
        String::from_utf8(output.into_inner()).unwrap(),
        concat!(
            "Pane ID: pane-1\n",
            "Title: shell\n",
            "Cwd: /tmp/project\n",
            "Focused: true\n",
            "Active: true\n",
            "Truncated: false\n",
            "Cursor Row: 12\n",
            "Cursor Col: 34\n",
            "\n",
            "$ pwd\n",
            "/tmp/project\n"
        )
    );
}

#[test]
fn terminal_control_read_text_output_uses_header_section_then_content() {
    let mut output = Cursor::new(Vec::new());

    write_response_to(
        &mut output,
        &TerminalControlResponse::ReadPane(sample_snapshot()),
        OutputFormat::Text,
    )
    .unwrap();

    assert_eq!(
        String::from_utf8(output.into_inner()).unwrap(),
        concat!(
            "Pane ID: pane-1\n",
            "Title: shell\n",
            "Cwd: /tmp/project\n",
            "Focused: true\n",
            "Active: true\n",
            "Truncated: false\n",
            "Cursor Row: 12\n",
            "Cursor Col: 34\n",
            "\n",
            "$ pwd\n",
            "/tmp/project\n"
        )
    );
}

#[test]
fn write_response_to_formats_create_tab_pretty() {
    let mut output = Cursor::new(Vec::new());

    write_response_to(
        &mut output,
        &TerminalControlResponse::CreateTab {
            pane_id: "pane-1".to_string(),
        },
        OutputFormat::Pretty,
    )
    .unwrap();

    assert_eq!(
        String::from_utf8(output.into_inner()).unwrap(),
        "Pane ID: pane-1\n"
    );
}

#[test]
fn write_response_to_formats_list_tabs_pretty() {
    let mut output = Cursor::new(Vec::new());

    write_response_to(
        &mut output,
        &TerminalControlResponse::ListTabs(vec![TabSummary {
            tab_id: "tab-1".to_string(),
            title: "shell".to_string(),
            active: true,
        }]),
        OutputFormat::Pretty,
    )
    .unwrap();

    let rendered = String::from_utf8(output.into_inner()).unwrap();
    assert!(rendered.contains("tab-1"));
    assert!(rendered.contains("shell"));
    assert!(rendered.contains("true"));
}

#[test]
fn write_response_to_formats_current_pane_pretty() {
    let mut output = Cursor::new(Vec::new());

    write_response_to(
        &mut output,
        &TerminalControlResponse::CurrentPane(sample_pane()),
        OutputFormat::Pretty,
    )
    .unwrap();

    assert_eq!(
        String::from_utf8(output.into_inner()).unwrap(),
        concat!(
            "Pane ID: pane-1\n",
            "Tab ID: tab-1\n",
            "Workspace ID: workspace-1\n",
            "Title: shell\n",
            "Cwd: /tmp/project\n",
            "Command: zsh\n",
            "Focused: true\n",
            "Active: true\n",
            "Pane type: terminal\n"
        )
    );
}

#[test]
fn write_response_to_formats_json_output() {
    let mut output = Cursor::new(Vec::new());

    write_response_to(
        &mut output,
        &TerminalControlResponse::CurrentPane(sample_pane()),
        OutputFormat::Json,
    )
    .unwrap();

    assert_eq!(
        String::from_utf8(output.into_inner()).unwrap(),
        format!(
            "{}\n",
            serde_json::to_string_pretty(&sample_pane()).unwrap()
        )
    );
}

#[test]
fn write_response_to_formats_success_messages_for_default_output() {
    let mut output = Cursor::new(Vec::new());

    write_response_to(
        &mut output,
        &TerminalControlResponse::SendText,
        OutputFormat::Pretty,
    )
    .unwrap();

    assert_eq!(
        String::from_utf8(output.into_inner()).unwrap(),
        "Text sent\n"
    );
}

#[test]
fn format_error_returns_concise_cli_messages() {
    assert_eq!(
        format_error(&TerminalControlError::PaneNotFound {
            pane_id: Some("pane-404".to_string()),
        }),
        "Pane not found: pane-404"
    );
    assert_eq!(
        format_error(&TerminalControlError::PaneNotTerminal {
            pane_id: "pane-123".to_string(),
        }),
        "Pane is not a terminal: pane-123"
    );
    assert_eq!(
        format_error(&TerminalControlError::PaneNotWritable {
            pane_id: "pane-123".to_string(),
        }),
        "Pane is not writable: pane-123"
    );
    assert_eq!(
        format_error(&TerminalControlError::NoActiveTerminal),
        "No active terminal"
    );
    assert_eq!(
        format_error(&TerminalControlError::UnsupportedKey {
            key: "f13".to_string(),
        }),
        "Unsupported key: f13"
    );
    assert_eq!(
        format_error(&TerminalControlError::IpcUnavailable),
        "Warp app is not available"
    );
}

#[test]
fn list_rows_serialize_as_expected() {
    let tab = TerminalControlListTabRow::from(TabSummary {
        tab_id: "tab-1".to_string(),
        title: "shell".to_string(),
        active: true,
    });
    let pane = TerminalControlListPaneRow::from(sample_pane());

    assert_eq!(
        serde_json::to_value(tab).unwrap(),
        json!({
            "tab_id": "tab-1",
            "title": "shell",
            "active": true,
        })
    );
    assert_eq!(
        serde_json::to_value(pane).unwrap(),
        json!({
            "pane_id": "pane-1",
            "tab_id": "tab-1",
            "workspace_id": "workspace-1",
            "title": "shell",
            "cwd": "/tmp/project",
            "command": "zsh",
            "focused": true,
            "active": true,
            "pane_type": "terminal",
        })
    );
}

fn sample_pane() -> PaneSummary {
    PaneSummary {
        pane_id: "pane-1".to_string(),
        tab_id: "tab-1".to_string(),
        workspace_id: Some("workspace-1".to_string()),
        title: "shell".to_string(),
        cwd: Some("/tmp/project".to_string()),
        command: Some("zsh".to_string()),
        focused: true,
        active: true,
        pane_type: "terminal".to_string(),
    }
}

fn sample_snapshot() -> TerminalPaneSnapshot {
    TerminalPaneSnapshot {
        pane_id: "pane-1".to_string(),
        title: "shell".to_string(),
        cwd: Some("/tmp/project".to_string()),
        focused: true,
        active: true,
        content: "$ pwd\n/tmp/project\n".to_string(),
        truncated: false,
        cursor_row: Some(12),
        cursor_col: Some(34),
    }
}
