# Terminal Control CLI Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a `cmux`-style Warp CLI and app-hosted IPC service that can list terminal panes and send text or keys to a target terminal pane without changing visible focus by default.

**Architecture:** The Warp app will host a typed `TerminalControlService` over existing `crates/ipc` transport, while new `warp_cli` subcommands provide a `cmux`-style user interface. The service will resolve terminal panes in-process through workspace and pane-group state, and route terminal control through a small non-focus-dependent PTY write API on `TerminalView`.

**Tech Stack:** Rust, clap, Warp app `agent_sdk` command dispatch, Warp `ipc`, WarpUI entity/view model, terminal PTY write path.

---

### Task 1: Add CLI Command Parsing Surface

**Files:**
- Create: `crates/warp_cli/src/terminal_control.rs`
- Modify: `crates/warp_cli/src/lib.rs`
- Test: `crates/warp_cli/src/lib_tests.rs`

- [ ] **Step 1: Write the failing CLI parsing tests**

Add tests to `crates/warp_cli/src/lib_tests.rs` covering:

```rust
#[test]
fn terminal_control_list_panes_parses() {
    let args = Args::try_parse_from(["warp", "list-panes"]).unwrap();

    let Some(Command::CommandLine(boxed_cmd)) = args.command else {
        panic!("Expected `warp list-panes` command");
    };

    assert!(matches!(
        boxed_cmd.as_ref(),
        CliCommand::TerminalControl(crate::terminal_control::TerminalControlCommand::ListPanes)
    ));
}

#[test]
fn terminal_control_send_pane_parses() {
    let args = Args::try_parse_from(["warp", "send-pane", "pane-123", "echo hi"])
        .unwrap();

    let Some(Command::CommandLine(boxed_cmd)) = args.command else {
        panic!("Expected `warp send-pane` command");
    };

    let CliCommand::TerminalControl(
        crate::terminal_control::TerminalControlCommand::SendPane(args),
    ) = boxed_cmd.as_ref()
    else {
        panic!("Expected `warp send-pane` command");
    };

    assert_eq!(args.pane_id, "pane-123");
    assert_eq!(args.text, "echo hi");
}

#[test]
fn terminal_control_send_key_pane_parses() {
    let args = Args::try_parse_from(["warp", "send-key-pane", "pane-123", "enter"])
        .unwrap();

    let Some(Command::CommandLine(boxed_cmd)) = args.command else {
        panic!("Expected `warp send-key-pane` command");
    };

    let CliCommand::TerminalControl(
        crate::terminal_control::TerminalControlCommand::SendKeyPane(args),
    ) = boxed_cmd.as_ref()
    else {
        panic!("Expected `warp send-key-pane` command");
    };

    assert_eq!(args.pane_id, "pane-123");
    assert_eq!(args.key, "enter");
}
```

- [ ] **Step 2: Run the CLI parsing tests to verify they fail**

Run: `cargo test -p warp_cli terminal_control_`
Expected: FAIL with missing `terminal_control` module or `CliCommand::TerminalControl` variant.

- [ ] **Step 3: Add the new clap command module and command enum**

Create `crates/warp_cli/src/terminal_control.rs` with:

```rust
use clap::Args;

#[derive(Debug, Clone, clap::Subcommand)]
pub enum TerminalControlCommand {
    ListTabs,
    ListPanes,
    CurrentPane,
    FocusPane(PaneTargetArgs),
    Send(SendTextArgs),
    SendPane(SendTextToPaneArgs),
    SendKey(SendKeyArgs),
    SendKeyPane(SendKeyToPaneArgs),
}

#[derive(Debug, Clone, Args)]
pub struct PaneTargetArgs {
    pub pane_id: String,
}

#[derive(Debug, Clone, Args)]
pub struct SendTextArgs {
    pub text: String,
}

#[derive(Debug, Clone, Args)]
pub struct SendTextToPaneArgs {
    pub pane_id: String,
    pub text: String,
}

#[derive(Debug, Clone, Args)]
pub struct SendKeyArgs {
    pub key: String,
}

#[derive(Debug, Clone, Args)]
pub struct SendKeyToPaneArgs {
    pub pane_id: String,
    pub key: String,
}
```

Modify `crates/warp_cli/src/lib.rs` to:

```rust
pub mod terminal_control;
```

and add a new `CliCommand` variant:

```rust
#[command(subcommand)]
TerminalControl(crate::terminal_control::TerminalControlCommand),
```

Also add command aliases so the top-level commands parse as requested:

```rust
#[command(name = "list-panes")]
#[command(name = "list-tabs")]
#[command(name = "current-pane")]
#[command(name = "focus-pane")]
#[command(name = "send")]
#[command(name = "send-pane")]
#[command(name = "send-key")]
#[command(name = "send-key-pane")]
```

If clap flattening requires a nested enum instead, keep the top-level UX by adding visible aliases on the command variant.

- [ ] **Step 4: Run the CLI parsing tests to verify they pass**

Run: `cargo test -p warp_cli terminal_control_`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/warp_cli/src/lib.rs crates/warp_cli/src/terminal_control.rs crates/warp_cli/src/lib_tests.rs
git commit -m "feat: add terminal control CLI parsing"
```

### Task 2: Add Agent SDK Dispatch for Terminal Control Commands

**Files:**
- Create: `app/src/ai/agent_sdk/terminal_control.rs`
- Modify: `app/src/ai/agent_sdk/mod.rs`
- Test: `app/src/ai/agent_sdk/mod_tests.rs`

- [ ] **Step 1: Write the failing dispatch and auth tests**

Add tests to `app/src/ai/agent_sdk/mod_tests.rs`:

```rust
use warp_cli::{
    terminal_control::{SendTextArgs, TerminalControlCommand},
    CliCommand,
};

#[test]
fn terminal_control_send_requires_auth() {
    assert!(command_requires_auth(&CliCommand::TerminalControl(
        TerminalControlCommand::Send(SendTextArgs {
            text: "pwd".to_string(),
        }),
    )));
}

#[test]
fn terminal_control_list_telemetry_maps() {
    let event = command_to_telemetry_event(&CliCommand::TerminalControl(
        TerminalControlCommand::ListPanes,
    ));

    assert_eq!(event.name(), "terminal_control_list_panes");
}
```

- [ ] **Step 2: Run the agent SDK tests to verify they fail**

Run: `cargo test -p warp_cli terminal_control_ && cargo test -p app terminal_control_`
Expected: FAIL in `app` tests because `CliCommand::TerminalControl` is not handled in auth or telemetry matches.

- [ ] **Step 3: Add command dispatch wiring**

Create `app/src/ai/agent_sdk/terminal_control.rs` with a stub entrypoint:

```rust
use anyhow::Result;
use warp_cli::{terminal_control::TerminalControlCommand, GlobalOptions};
use warpui::AppContext;

pub fn run(
    _ctx: &mut AppContext,
    _global_options: GlobalOptions,
    _command: TerminalControlCommand,
) -> Result<()> {
    Err(anyhow::anyhow!("terminal control not implemented yet"))
}
```

Modify `app/src/ai/agent_sdk/mod.rs` to:
- `mod terminal_control;`
- dispatch `CliCommand::TerminalControl(cmd) => terminal_control::run(ctx, global_options, cmd)`
- add `CliCommand::TerminalControl(_) => true` in `command_requires_auth`
- add telemetry mapping for each terminal control command

If `CliTelemetryEvent` needs new variants, add them in the same task.

- [ ] **Step 4: Run the agent SDK tests to verify they pass**

Run: `cargo test -p app terminal_control_`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add app/src/ai/agent_sdk/mod.rs app/src/ai/agent_sdk/terminal_control.rs app/src/ai/agent_sdk/mod_tests.rs
git commit -m "feat: wire terminal control commands into CLI dispatch"
```

### Task 3: Define the Terminal Control IPC Contract

**Files:**
- Create: `app/src/app_services/terminal_control_service.rs`
- Modify: `app/src/app_services/mod.rs`
- Test: `app/src/app_services/terminal_control_service_tests.rs`

- [ ] **Step 1: Write the failing IPC contract tests**

Create `app/src/app_services/terminal_control_service_tests.rs` with:

```rust
use super::terminal_control_service::{
    PaneTarget, PaneTargetRequest, SendKeyRequest, SendTextRequest, TerminalControlService,
};

#[test]
fn terminal_control_requests_round_trip_with_bincode() {
    let request = SendTextRequest {
        target: PaneTarget::PaneId("pane-123".to_string()),
        text: "echo hi".to_string(),
    };

    let bytes = bincode::serialize(&request).unwrap();
    let decoded: SendTextRequest = bincode::deserialize(&bytes).unwrap();

    assert_eq!(decoded.text, "echo hi");
    assert!(matches!(decoded.target, PaneTarget::PaneId(id) if id == "pane-123"));
}

#[test]
fn terminal_control_service_has_stable_request_types() {
    let _: Option<<TerminalControlService as ipc::Service>::Request> = None;
    let _: Option<<TerminalControlService as ipc::Service>::Response> = None;
}
```

- [ ] **Step 2: Run the IPC contract tests to verify they fail**

Run: `cargo test -p app terminal_control_service_`
Expected: FAIL because the service module does not exist.

- [ ] **Step 3: Add the typed IPC contract**

Create `app/src/app_services/terminal_control_service.rs` with:

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum PaneTarget {
    ActivePane,
    PaneId(String),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TabSummary {
    pub tab_id: String,
    pub window_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PaneSummary {
    pub pane_id: String,
    pub tab_id: String,
    pub window_id: String,
    pub title: String,
    pub cwd: Option<String>,
    pub command: Option<String>,
    pub focused: bool,
    pub active: bool,
    pub pane_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum TerminalControlRequest {
    ListTabs,
    ListPanes,
    CurrentPane,
    FocusPane { target: PaneTarget },
    SendText(SendTextRequest),
    SendKey(SendKeyRequest),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SendTextRequest {
    pub target: PaneTarget,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SendKeyRequest {
    pub target: PaneTarget,
    pub key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum TerminalControlError {
    PaneNotFound,
    PaneNotTerminal,
    PaneNotWritable,
    NoActiveTerminal,
    UnsupportedKey,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum TerminalControlResponse {
    Tabs(Vec<TabSummary>),
    Panes(Vec<PaneSummary>),
    CurrentPane(Option<PaneSummary>),
    Ok,
    Err(TerminalControlError),
}

pub struct TerminalControlService;

impl ipc::Service for TerminalControlService {
    type Request = TerminalControlRequest;
    type Response = TerminalControlResponse;
}
```

Modify `app/src/app_services/mod.rs` to declare the new module and include the test module at the end of the file.

- [ ] **Step 4: Run the IPC contract tests to verify they pass**

Run: `cargo test -p app terminal_control_service_`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add app/src/app_services/mod.rs app/src/app_services/terminal_control_service.rs app/src/app_services/terminal_control_service_tests.rs
git commit -m "feat: add terminal control IPC contract"
```

### Task 4: Add Non-Focusing Pane Lookup and Programmatic Terminal Writes

**Files:**
- Modify: `app/src/workspace/view.rs`
- Modify: `app/src/terminal/view.rs`
- Test: `app/src/workspace/view_test.rs`
- Test: `app/src/terminal/view_test.rs`

- [ ] **Step 1: Write the failing pane lookup test**

Add a test to `app/src/workspace/view_test.rs` that verifies explicit pane lookup does not change focus:

```rust
#[test]
fn find_terminal_pane_locator_does_not_change_focused_pane() {
    // Build a workspace with two terminal panes.
    // Capture the initially focused pane.
    // Resolve the second pane through the new lookup helper.
    // Assert the helper returns the expected locator.
    // Assert focused pane remains unchanged.
}
```

- [ ] **Step 2: Run the pane lookup test to verify it fails**

Run: `cargo test -p app find_terminal_pane_locator_does_not_change_focused_pane`
Expected: FAIL because the helper does not exist.

- [ ] **Step 3: Add the non-focusing lookup helper**

Modify `app/src/workspace/view.rs` to add a helper with this shape:

```rust
pub fn find_terminal_pane_locator(
    &self,
    pane_id: PaneId,
    ctx: &AppContext,
) -> Option<PaneViewLocator> {
    self.tabs.iter().find_map(|tab| {
        let pane_group = tab.pane_group.as_ref(ctx);
        if !pane_group.has_pane_id(pane_id) {
            return None;
        }

        pane_group.terminal_view_from_pane_id(pane_id, ctx)?;

        Some(PaneViewLocator {
            pane_group_id: tab.pane_group.id(),
            pane_id,
        })
    })
}
```

Keep it lookup-only. Do not call `focus_pane`.

- [ ] **Step 4: Write the failing programmatic terminal write test**

Add a test to `app/src/terminal/view_test.rs` that exercises a new programmatic write entrypoint for a non-focused terminal view:

```rust
#[test]
fn programmatic_write_to_pty_uses_existing_write_path() {
    // Create terminal view state.
    // Call the new programmatic write helper.
    // Assert the expected PTY write event is emitted or queued.
}
```

- [ ] **Step 5: Run the terminal write test to verify it fails**

Run: `cargo test -p app programmatic_write_to_pty_uses_existing_write_path`
Expected: FAIL because the helper does not exist.

- [ ] **Step 6: Add the programmatic PTY write helper**

Modify `app/src/terminal/view.rs` to add a small helper that avoids user typing and focus-only logic:

```rust
pub fn write_programmatic_bytes_to_pty<B: Into<Cow<'static, [u8]>>>(
    &mut self,
    data: B,
    ctx: &mut ViewContext<Self>,
) {
    self.write_to_pty(data, ctx);
}
```

If `send_key` support needs a second helper, add:

```rust
pub fn write_programmatic_key_sequence_to_pty(
    &mut self,
    bytes: Vec<u8>,
    ctx: &mut ViewContext<Self>,
) {
    self.write_programmatic_bytes_to_pty(bytes, ctx);
}
```

Do not add new `model.lock()` calls unless required. Reuse existing write plumbing.

- [ ] **Step 7: Run both tests to verify they pass**

Run: `cargo test -p app find_terminal_pane_locator_does_not_change_focused_pane programmatic_write_to_pty_uses_existing_write_path`
Expected: PASS

- [ ] **Step 8: Commit**

```bash
git add app/src/workspace/view.rs app/src/workspace/view_test.rs app/src/terminal/view.rs app/src/terminal/view_test.rs
git commit -m "feat: add non-focusing terminal lookup and write hooks"
```

### Task 5: Implement the App-Hosted Terminal Control Service

**Files:**
- Modify: `app/src/app_services/mod.rs`
- Modify: `app/src/app_services/windows/mod.rs`
- Modify: `app/src/app_services/windows/service_impl.rs`
- Modify: `app/src/app_services/windows/single_instance_manager.rs`
- Modify: `app/src/app_services/linux/mod.rs`
- Modify: `app/src/app_services/terminal_control_service.rs`
- Modify: `app/src/workspace/registry.rs`
- Modify: `app/src/workspace/view.rs`
- Test: `app/src/app_services/terminal_control_service_tests.rs`

- [ ] **Step 1: Write the failing service behavior tests**

Extend `app/src/app_services/terminal_control_service_tests.rs` with:

```rust
#[test]
fn send_text_to_missing_pane_returns_pane_not_found() {
    // Construct the service impl with no matching pane.
    // Assert the response is Err(PaneNotFound).
}

#[test]
fn send_key_with_unknown_key_returns_unsupported_key() {
    // Call the key translation path with an unsupported key.
    // Assert the response is Err(UnsupportedKey).
}
```

- [ ] **Step 2: Run the service behavior tests to verify they fail**

Run: `cargo test -p app send_text_to_missing_pane_returns_pane_not_found send_key_with_unknown_key_returns_unsupported_key`
Expected: FAIL because there is no service implementation.

- [ ] **Step 3: Add the service implementation and key translation**

Modify `app/src/app_services/terminal_control_service.rs` to add:

```rust
use async_trait::async_trait;

#[derive(Clone)]
pub struct TerminalControlServiceImpl;

#[async_trait]
impl ipc::ServiceImpl for TerminalControlServiceImpl {
    type Service = TerminalControlService;

    async fn handle_request(&self, request: TerminalControlRequest) -> TerminalControlResponse {
        // Delegate into the app-owned dispatch path.
        TerminalControlResponse::Err(TerminalControlError::PaneNotFound)
    }
}
```

Replace the stub body with real logic that:
- lists tabs and panes from registered workspaces
- resolves `ActivePane` or `PaneId`
- focuses only for `FocusPane`
- writes bytes through `TerminalView::write_programmatic_bytes_to_pty`
- translates supported keys (`enter`, `tab`, `esc`, `backspace`, arrows) into terminal bytes

If the app service host requires platform-specific wiring, follow the existing patterns:
- Windows fixed-address IPC in `app_services/windows`
- Linux app-service host in `app_services/linux`

Add the smallest shared bootstrap needed in `app_services/mod.rs` so the running app owns the service.

- [ ] **Step 4: Run the service behavior tests to verify they pass**

Run: `cargo test -p app terminal_control_service_ send_text_to_missing_pane_returns_pane_not_found send_key_with_unknown_key_returns_unsupported_key`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add app/src/app_services/mod.rs app/src/app_services/windows/mod.rs app/src/app_services/windows/service_impl.rs app/src/app_services/windows/single_instance_manager.rs app/src/app_services/linux/mod.rs app/src/app_services/terminal_control_service.rs app/src/workspace/registry.rs app/src/workspace/view.rs app/src/app_services/terminal_control_service_tests.rs
git commit -m "feat: host terminal control service in the app"
```

### Task 6: Connect the CLI to the Terminal Control Service

**Files:**
- Modify: `app/src/ai/agent_sdk/terminal_control.rs`
- Modify: `app/src/app_services/terminal_control_service.rs`
- Test: `app/src/ai/agent_sdk/mod_tests.rs`

- [ ] **Step 1: Write the failing CLI-to-service tests**

Add tests covering request construction and output formatting:

```rust
#[test]
fn terminal_control_send_builds_active_pane_request() {
    // Assert `warp send "pwd"` maps to SendText { target: ActivePane, text: "pwd" }.
}

#[test]
fn terminal_control_send_pane_builds_explicit_target_request() {
    // Assert `warp send-pane pane-123 "pwd"` maps to PaneId("pane-123").
}
```

- [ ] **Step 2: Run the CLI-to-service tests to verify they fail**

Run: `cargo test -p app terminal_control_send_builds_`
Expected: FAIL because the command runner still returns `not implemented`.

- [ ] **Step 3: Implement the IPC client and output formatting**

Modify `app/src/ai/agent_sdk/terminal_control.rs` to:
- connect to the running app via `ipc::Client::connect`
- create `ipc::service_caller::<TerminalControlService>`
- map each CLI command to a typed request
- print list output in pretty format and JSON format according to `global_options.output_format`
- surface structured service errors as concise CLI errors

Use the same background executor pattern as `forward_uri_to_sole_running_instance` if no `AppContext` executor is available at connection time.

- [ ] **Step 4: Run the CLI-to-service tests to verify they pass**

Run: `cargo test -p app terminal_control_`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add app/src/ai/agent_sdk/terminal_control.rs app/src/ai/agent_sdk/mod_tests.rs app/src/app_services/terminal_control_service.rs
git commit -m "feat: connect terminal control CLI to app IPC service"
```

### Task 7: Final Verification and Cleanup

**Files:**
- Modify: any files touched above if verification reveals issues

- [ ] **Step 1: Format the changed Rust files**

Run: `cargo fmt --all`
Expected: formatting completes successfully

- [ ] **Step 2: Run targeted tests for changed crates**

Run: `cargo test -p warp_cli terminal_control_`
Expected: PASS

Run: `cargo test -p app terminal_control_`
Expected: PASS if the local machine has the required toolchain; otherwise document the environment blocker and keep the failing command output.

- [ ] **Step 3: Run the narrowest practical clippy or check command**

Run: `cargo check -p warp_cli`
Expected: PASS

Run: `cargo check -p app`
Expected: PASS if the local machine has the required toolchain; otherwise document the Metal toolchain blocker.

- [ ] **Step 4: Review git status**

Run: `git status --short`
Expected: only intended plan, spec, and implementation files are modified.

- [ ] **Step 5: Commit**

```bash
git add docs/superpowers/specs/2026-05-02-terminal-control-design.md docs/superpowers/plans/2026-05-02-terminal-control-cli.md .
git commit -m "feat: add terminal control CLI and IPC service"
```
