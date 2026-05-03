use std::sync::Arc;

use anyhow::Context;
use comfy_table::Cell;
use serde::Serialize;
use warp_cli::{
    GlobalOptions,
    agent::OutputFormat,
    terminal_control::{
        PaneTargetArgs, SendKeyArgs, SendKeyToPaneArgs, SendTextArgs, SendTextToPaneArgs,
        TerminalControlCommand,
    },
};
use warpui::{AppContext, r#async::block_on};

use crate::app_services::terminal_control_service::{
    PaneSummary, TabSummary, TerminalControlError, TerminalControlRequest, TerminalControlResponse,
    TerminalControlService, TerminalControlTarget, TerminalPaneSnapshot,
    terminal_control_service_address,
};

use super::output::{self, TableFormat};

pub fn run(
    ctx: &mut AppContext,
    global_options: GlobalOptions,
    command: TerminalControlCommand,
) -> anyhow::Result<()> {
    let request = command_to_request(command)?;
    let response = call_service(ctx, request)?;
    let mut stdout = std::io::stdout();
    write_response_to(&mut stdout, &response, global_options.output_format)
}

fn call_service(
    ctx: &AppContext,
    request: TerminalControlRequest,
) -> anyhow::Result<TerminalControlResponse> {
    let background_executor = ctx.background_executor().clone();

    let response = block_on(async move {
        let client = ipc::Client::connect(
            terminal_control_service_address().into(),
            background_executor,
        )
        .await
        .map_err(|_| TerminalControlError::IpcUnavailable)?;

        ipc::service_caller::<TerminalControlService>(Arc::new(client))
            .call(request)
            .await
            .map_err(|_| TerminalControlError::IpcUnavailable)
    })
    .map_err(|err| anyhow::anyhow!(format_error(&err)))?;

    match response {
        TerminalControlResponse::Error(err) => Err(anyhow::anyhow!(format_error(&err))),
        other => Ok(other),
    }
}

fn command_to_request(command: TerminalControlCommand) -> anyhow::Result<TerminalControlRequest> {
    match command {
        TerminalControlCommand::ListTabs => Ok(TerminalControlRequest::ListTabs),
        TerminalControlCommand::ListPanes => Ok(TerminalControlRequest::ListPanes),
        TerminalControlCommand::CurrentPane => Ok(TerminalControlRequest::CurrentPane),
        TerminalControlCommand::Read => Ok(TerminalControlRequest::ReadPane {
            target: TerminalControlTarget::ActivePane,
        }),
        TerminalControlCommand::ReadPane(PaneTargetArgs { pane_id }) => {
            Ok(TerminalControlRequest::ReadPane {
                target: TerminalControlTarget::PaneId(pane_id),
            })
        }
        TerminalControlCommand::FocusPane(PaneTargetArgs { pane_id }) => {
            Ok(TerminalControlRequest::FocusPane {
                target: TerminalControlTarget::PaneId(pane_id),
            })
        }
        TerminalControlCommand::Send(SendTextArgs { text }) => Ok(TerminalControlRequest::SendText {
            target: TerminalControlTarget::ActivePane,
            text,
        }),
        TerminalControlCommand::SendPane(SendTextToPaneArgs { pane_id, text }) => {
            Ok(TerminalControlRequest::SendText {
                target: TerminalControlTarget::PaneId(pane_id),
                text,
            })
        }
        TerminalControlCommand::SendKey(SendKeyArgs { key }) => Ok(TerminalControlRequest::SendKey {
            target: TerminalControlTarget::ActivePane,
            key,
        }),
        TerminalControlCommand::SendKeyPane(SendKeyToPaneArgs { pane_id, key }) => {
            Ok(TerminalControlRequest::SendKey {
                target: TerminalControlTarget::PaneId(pane_id),
                key,
            })
        }
    }
}

fn write_response_to<W: std::io::Write>(
    output: &mut W,
    response: &TerminalControlResponse,
    output_format: OutputFormat,
) -> anyhow::Result<()> {
    match response {
        TerminalControlResponse::ListTabs(tabs) => {
            let rows = tabs.iter().cloned().map(TerminalControlListTabRow::from);
            output::write_list(rows, output_format, output)
        }
        TerminalControlResponse::ListPanes(panes) => {
            let rows = panes.iter().cloned().map(TerminalControlListPaneRow::from);
            output::write_list(rows, output_format, output)
        }
        TerminalControlResponse::CurrentPane(pane) => {
            write_current_pane_to(output, pane, output_format)
        }
        TerminalControlResponse::ReadPane(snapshot) => {
            write_read_pane_to(output, snapshot, output_format)
        }
        TerminalControlResponse::FocusPane => {
            write_success_message_to(output, output_format, "Pane focused")
        }
        TerminalControlResponse::SendText => {
            write_success_message_to(output, output_format, "Text sent")
        }
        TerminalControlResponse::SendKey => {
            write_success_message_to(output, output_format, "Key sent")
        }
        TerminalControlResponse::Error(err) => Err(anyhow::anyhow!(format_error(err))),
    }
}

fn write_read_pane_to<W: std::io::Write>(
    output: &mut W,
    snapshot: &TerminalPaneSnapshot,
    output_format: OutputFormat,
) -> anyhow::Result<()> {
    match output_format {
        OutputFormat::Json => output::write_json(snapshot, output),
        OutputFormat::Ndjson => output::write_json_line(snapshot, output),
        OutputFormat::Pretty | OutputFormat::Text => {
            writeln!(output, "Pane ID: {}", snapshot.pane_id)?;
            writeln!(output, "Title: {}", snapshot.title)?;
            writeln!(output, "Cwd: {}", snapshot.cwd.as_deref().unwrap_or(""))?;
            writeln!(output, "Focused: {}", snapshot.focused)?;
            writeln!(output, "Active: {}", snapshot.active)?;
            writeln!(output, "Truncated: {}", snapshot.truncated)?;
            writeln!(
                output,
                "Cursor Row: {}",
                snapshot
                    .cursor_row
                    .map(|row| row.to_string())
                    .as_deref()
                    .unwrap_or("")
            )?;
            writeln!(
                output,
                "Cursor Col: {}",
                snapshot
                    .cursor_col
                    .map(|col| col.to_string())
                    .as_deref()
                    .unwrap_or("")
            )?;
            writeln!(output)?;
            write!(output, "{}", snapshot.content)?;
            Ok(())
        }
    }
}

fn write_current_pane_to<W: std::io::Write>(
    output: &mut W,
    pane: &PaneSummary,
    output_format: OutputFormat,
) -> anyhow::Result<()> {
    match output_format {
        OutputFormat::Json => output::write_json(pane, output),
        OutputFormat::Ndjson => output::write_json_line(pane, output),
        OutputFormat::Pretty => {
            writeln!(output, "Pane ID: {}", pane.pane_id)?;
            writeln!(output, "Tab ID: {}", pane.tab_id)?;
            writeln!(
                output,
                "Workspace ID: {}",
                pane.workspace_id.as_deref().unwrap_or("")
            )?;
            writeln!(output, "Title: {}", pane.title)?;
            writeln!(output, "Cwd: {}", pane.cwd.as_deref().unwrap_or(""))?;
            writeln!(output, "Command: {}", pane.command.as_deref().unwrap_or(""))?;
            writeln!(output, "Focused: {}", pane.focused)?;
            writeln!(output, "Active: {}", pane.active)?;
            writeln!(output, "Pane type: {}", pane.pane_type)?;
            Ok(())
        }
        OutputFormat::Text => {
            writeln!(
                output,
                "Pane ID\tTab ID\tWorkspace ID\tTitle\tCwd\tCommand\tFocused\tActive\tPane type"
            )?;
            writeln!(
                output,
                "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
                pane.pane_id,
                pane.tab_id,
                pane.workspace_id.as_deref().unwrap_or(""),
                pane.title,
                pane.cwd.as_deref().unwrap_or(""),
                pane.command.as_deref().unwrap_or(""),
                pane.focused,
                pane.active,
                pane.pane_type
            )?;
            Ok(())
        }
    }
}

fn write_success_message_to<W: std::io::Write>(
    output: &mut W,
    output_format: OutputFormat,
    message: &str,
) -> anyhow::Result<()> {
    #[derive(Serialize)]
    struct SuccessOutput<'a> {
        message: &'a str,
    }

    match output_format {
        OutputFormat::Json => output::write_json(&SuccessOutput { message }, output),
        OutputFormat::Ndjson => output::write_json_line(&SuccessOutput { message }, output),
        OutputFormat::Pretty | OutputFormat::Text => {
            writeln!(output, "{message}").context("unable to write terminal control output")
        }
    }
}

fn format_error(error: &TerminalControlError) -> String {
    match error {
        TerminalControlError::PaneNotFound {
            pane_id: Some(pane_id),
        } => format!("Pane not found: {pane_id}"),
        TerminalControlError::PaneNotFound { pane_id: None } => "Pane not found".to_string(),
        TerminalControlError::PaneNotTerminal { pane_id } => {
            format!("Pane is not a terminal: {pane_id}")
        }
        TerminalControlError::PaneNotReadable { pane_id } => {
            format!("Pane is not readable: {pane_id}")
        }
        TerminalControlError::PaneNotWritable { pane_id } => {
            format!("Pane is not writable: {pane_id}")
        }
        TerminalControlError::NoActiveTerminal => "No active terminal".to_string(),
        TerminalControlError::UnsupportedKey { key } => format!("Unsupported key: {key}"),
        TerminalControlError::IpcUnavailable => "Warp app is not available".to_string(),
    }
}

#[derive(Debug, Clone, Serialize)]
struct TerminalControlListTabRow {
    tab_id: String,
    title: String,
    active: bool,
}

impl From<TabSummary> for TerminalControlListTabRow {
    fn from(value: TabSummary) -> Self {
        Self {
            tab_id: value.tab_id,
            title: value.title,
            active: value.active,
        }
    }
}

impl TableFormat for TerminalControlListTabRow {
    fn header() -> Vec<Cell> {
        vec![Cell::new("Tab ID"), Cell::new("Title"), Cell::new("Active")]
    }

    fn row(&self) -> Vec<Cell> {
        vec![
            Cell::new(&self.tab_id),
            Cell::new(&self.title),
            Cell::new(self.active),
        ]
    }
}

#[derive(Debug, Clone, Serialize)]
struct TerminalControlListPaneRow {
    pane_id: String,
    tab_id: String,
    workspace_id: Option<String>,
    title: String,
    cwd: Option<String>,
    command: Option<String>,
    focused: bool,
    active: bool,
    pane_type: String,
}

impl From<PaneSummary> for TerminalControlListPaneRow {
    fn from(value: PaneSummary) -> Self {
        Self {
            pane_id: value.pane_id,
            tab_id: value.tab_id,
            workspace_id: value.workspace_id,
            title: value.title,
            cwd: value.cwd,
            command: value.command,
            focused: value.focused,
            active: value.active,
            pane_type: value.pane_type,
        }
    }
}

impl TableFormat for TerminalControlListPaneRow {
    fn header() -> Vec<Cell> {
        vec![
            Cell::new("Pane ID"),
            Cell::new("Tab ID"),
            Cell::new("Workspace ID"),
            Cell::new("Title"),
            Cell::new("Cwd"),
            Cell::new("Command"),
            Cell::new("Focused"),
            Cell::new("Active"),
            Cell::new("Pane type"),
        ]
    }

    fn row(&self) -> Vec<Cell> {
        vec![
            Cell::new(&self.pane_id),
            Cell::new(&self.tab_id),
            Cell::new(self.workspace_id.as_deref().unwrap_or("")),
            Cell::new(&self.title),
            Cell::new(self.cwd.as_deref().unwrap_or("")),
            Cell::new(self.command.as_deref().unwrap_or("")),
            Cell::new(self.focused),
            Cell::new(self.active),
            Cell::new(&self.pane_type),
        ]
    }
}

#[cfg(test)]
#[path = "terminal_control_tests.rs"]
mod tests;
