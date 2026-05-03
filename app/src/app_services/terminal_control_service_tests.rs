use std::any::TypeId;
use std::cell::RefCell;
use std::rc::Rc;

use super::{
    PaneSummary, TabSummary, TerminalControlError, TerminalControlRequest, TerminalControlResponse,
    TerminalControlService, TerminalControlTarget, TerminalPaneSnapshot,
};
use crate::notebooks::notebook::NotebookView;
use crate::pane_group::{Direction, NotebookPane, PaneGroupAction};
use crate::root_view;
use crate::terminal::view::{Event as TerminalEvent, TerminalView};
use crate::workspace::view::Workspace;
use warpui::SingletonEntity;
use warpui::windowing::WindowManager;
use warpui::windowing::state::ApplicationStage;
use warpui::{App, TypedActionView, ViewHandle, platform::WindowStyle};

fn round_trip<T>(value: &T) -> T
where
    T: serde::Serialize + serde::de::DeserializeOwned,
{
    let json = serde_json::to_string(value).expect("serialize test value");
    serde_json::from_str(&json).expect("deserialize test value")
}

fn mock_workspace(app: &mut App) -> ViewHandle<Workspace> {
    let global_resource_handles = crate::GlobalResourceHandles::mock(app);
    let active_window_id = app.read(|ctx| ctx.windows().active_window());
    let (_, workspace) = app.add_window(WindowStyle::NotStealFocus, |ctx| {
        Workspace::new(
            global_resource_handles,
            None,
            crate::root_view::NewWorkspaceSource::Empty {
                previous_active_window: active_window_id,
                shell: None,
            },
            ctx,
        )
    });
    workspace
}

fn mock_workspace_viewing_shared_session(app: &mut App) -> ViewHandle<Workspace> {
    let global_resource_handles = crate::GlobalResourceHandles::mock(app);
    let session_id = session_sharing_protocol::common::SessionId::new();

    let (_, workspace) = app.add_window(WindowStyle::NotStealFocus, |ctx| {
        Workspace::new(
            global_resource_handles,
            None,
            crate::root_view::NewWorkspaceSource::SharedSessionAsViewer { session_id },
            ctx,
        )
    });

    workspace
}

fn subscribe_to_pty_writes(
    app: &mut App,
    terminal: &ViewHandle<TerminalView>,
) -> Rc<RefCell<Vec<Vec<u8>>>> {
    let pty_writes: Rc<RefCell<Vec<Vec<u8>>>> = Rc::new(RefCell::new(Vec::new()));
    let writes = pty_writes.clone();
    app.update(|ctx| {
        ctx.subscribe_to_view(terminal, move |_, event, _| {
            if let TerminalEvent::WriteBytesToPty { bytes } = event {
                writes.borrow_mut().push(bytes.to_vec());
            }
        });
    });
    pty_writes
}

fn initialize_app(app: &mut App) {
    crate::workspace::view::tests::initialize_app(app);
    app.update(root_view::init);
}

#[test]
fn terminal_control_service_implements_ipc_service() {
    fn assert_service<S: ipc::Service>() {}

    assert_service::<TerminalControlService>();
    assert_eq!(
        TypeId::of::<<TerminalControlService as ipc::Service>::Request>(),
        TypeId::of::<TerminalControlRequest>()
    );
    assert_eq!(
        TypeId::of::<<TerminalControlService as ipc::Service>::Response>(),
        TypeId::of::<TerminalControlResponse>()
    );
}

#[test]
fn terminal_control_service_request_round_trips_all_operations() {
    let requests = vec![
        TerminalControlRequest::ListTabs,
        TerminalControlRequest::ListPanes,
        TerminalControlRequest::CurrentPane,
        TerminalControlRequest::FocusPane {
            target: TerminalControlTarget::PaneId("pane-123".to_string()),
        },
        TerminalControlRequest::SendText {
            target: TerminalControlTarget::ActivePane,
            text: "echo hi".to_string(),
        },
        TerminalControlRequest::SendKey {
            target: TerminalControlTarget::PaneId("pane-456".to_string()),
            key: "enter".to_string(),
        },
    ];

    for request in requests {
        assert_eq!(round_trip(&request), request);
    }
}

#[test]
fn terminal_control_service_read_request_round_trips() {
    let request = TerminalControlRequest::ReadPane {
        target: TerminalControlTarget::PaneId("pane-handle-123".to_string()),
    };

    assert_eq!(round_trip(&request), request);
}

#[test]
fn terminal_control_service_create_tab_request_round_trips() {
    let request = TerminalControlRequest::CreateTab;

    assert_eq!(round_trip(&request), request);
}

#[test]
fn terminal_control_service_read_response_round_trips() {
    let response = TerminalControlResponse::ReadPane(TerminalPaneSnapshot {
        pane_id: "pane-handle-123".to_string(),
        title: "shell".to_string(),
        cwd: Some("/tmp".to_string()),
        focused: true,
        active: true,
        content: "pwd\n/tmp\n".to_string(),
        truncated: false,
        cursor_row: None,
        cursor_col: None,
    });

    assert_eq!(round_trip(&response), response);
}

#[test]
fn terminal_control_service_create_tab_response_round_trips() {
    let response = TerminalControlResponse::CreateTab {
        pane_id: "pane-123".to_string(),
    };

    assert_eq!(round_trip(&response), response);
}

#[test]
fn terminal_control_service_create_tab_returns_new_initial_pane_handle() {
    App::test((), |mut app| async move {
        initialize_app(&mut app);
        let workspace = mock_workspace(&mut app);
        let window_id = app.read(|ctx| workspace.window_id(ctx));

        app.update(|ctx| {
            WindowManager::handle(ctx).update(ctx, |windowing_state: &mut WindowManager, _ctx| {
                windowing_state.overwrite_for_test(ApplicationStage::Active, Some(window_id));
            });
        });

        let existing_pane_ids = match TerminalControlService::handle_for_test(
            TerminalControlRequest::ListPanes,
            &mut app,
        ) {
            TerminalControlResponse::ListPanes(panes) => panes
                .into_iter()
                .map(|pane| pane.pane_id)
                .collect::<std::collections::HashSet<_>>(),
            other => panic!("expected pane listing, got {other:?}"),
        };

        let response =
            TerminalControlService::handle_for_test(TerminalControlRequest::CreateTab, &mut app);

        let new_pane_id = match response {
            TerminalControlResponse::CreateTab { pane_id } => pane_id,
            other => panic!("expected CreateTab response, got {other:?}"),
        };

        match TerminalControlService::handle_for_test(TerminalControlRequest::ListPanes, &mut app)
        {
            TerminalControlResponse::ListPanes(panes) => {
                assert!(panes.iter().any(|pane| pane.pane_id == new_pane_id));
                assert!(!existing_pane_ids.contains(&new_pane_id));
            }
            other => panic!("expected pane listing, got {other:?}"),
        }
    });
}

#[test]
fn terminal_control_service_current_pane_serializes_as_pane_summary() {
    let response = TerminalControlResponse::CurrentPane(PaneSummary {
        pane_id: "pane-1".to_string(),
        tab_id: "tab-1".to_string(),
        workspace_id: Some("workspace-1".to_string()),
        title: "shell".to_string(),
        cwd: Some("/tmp".to_string()),
        command: Some("zsh".to_string()),
        focused: true,
        active: true,
        pane_type: "terminal".to_string(),
    });

    let json = serde_json::to_value(&response).expect("serialize current pane response");

    assert_eq!(
        json,
        serde_json::json!({
            "CurrentPane": {
                "pane_id": "pane-1",
                "tab_id": "tab-1",
                "workspace_id": "workspace-1",
                "title": "shell",
                "cwd": "/tmp",
                "command": "zsh",
                "focused": true,
                "active": true,
                "pane_type": "terminal"
            }
        })
    );
}

#[test]
fn terminal_control_service_success_responses_serialize_as_unit_variants() {
    let responses = [
        TerminalControlResponse::FocusPane,
        TerminalControlResponse::SendText,
        TerminalControlResponse::SendKey,
    ];

    let expected_json = [
        serde_json::json!("FocusPane"),
        serde_json::json!("SendText"),
        serde_json::json!("SendKey"),
    ];

    for (response, expected_json) in responses.into_iter().zip(expected_json) {
        let json = serde_json::to_value(&response).expect("serialize success response");
        assert_eq!(json, expected_json);
        assert_eq!(round_trip(&response), response);
    }
}

#[test]
fn terminal_control_service_list_responses_round_trip() {
    let responses = [
        TerminalControlResponse::ListTabs(vec![TabSummary {
            tab_id: "tab-1".to_string(),
            title: "shell".to_string(),
            active: true,
        }]),
        TerminalControlResponse::ListPanes(vec![PaneSummary {
            pane_id: "pane-1".to_string(),
            tab_id: "tab-1".to_string(),
            workspace_id: Some("workspace-1".to_string()),
            title: "shell".to_string(),
            cwd: Some("/tmp".to_string()),
            command: Some("zsh".to_string()),
            focused: true,
            active: true,
            pane_type: "terminal".to_string(),
        }]),
        TerminalControlResponse::Error(TerminalControlError::UnsupportedKey {
            key: "f13".to_string(),
        }),
    ];

    for response in responses {
        assert_eq!(round_trip(&response), response);
    }
}

#[test]
fn terminal_control_service_current_pane_no_active_terminal_is_an_error() {
    let response = TerminalControlResponse::Error(TerminalControlError::NoActiveTerminal);

    let json = serde_json::to_value(&response).expect("serialize no active terminal error");

    assert_eq!(json, serde_json::json!({ "Error": "NoActiveTerminal" }));
}

#[test]
fn terminal_control_service_error_round_trips() {
    let errors = vec![
        TerminalControlError::PaneNotFound {
            pane_id: Some("pane-404".to_string()),
        },
        TerminalControlError::PaneNotTerminal {
            pane_id: "pane-123".to_string(),
        },
        TerminalControlError::PaneNotWritable {
            pane_id: "pane-123".to_string(),
        },
        TerminalControlError::NoActiveTerminal,
        TerminalControlError::UnsupportedKey {
            key: "f13".to_string(),
        },
        TerminalControlError::IpcUnavailable,
    ];

    for error in errors {
        assert_eq!(round_trip(&error), error);
    }
}

#[test]
fn terminal_control_service_list_and_current_pane_requests_need_runtime_impl() {
    App::test((), |mut app| async move {
        initialize_app(&mut app);
        let workspace = mock_workspace(&mut app);
        let window_id = app.read(|ctx| workspace.window_id(ctx));

        app.update(|ctx| {
            WindowManager::handle(ctx).update(ctx, |windowing_state: &mut WindowManager, _ctx| {
                windowing_state.overwrite_for_test(ApplicationStage::Active, Some(window_id));
            });
        });

        workspace.update(&mut app, |workspace, ctx| {
            workspace.add_terminal_tab(false, ctx);
            let active_tab = workspace.active_tab_pane_group().clone();
            active_tab.update(ctx, |pane_group, ctx| {
                pane_group.handle_action(&PaneGroupAction::Add(Direction::Right), ctx);
            });
        });

        workspace.update(&mut app, |workspace, ctx| {
            let pane_group = workspace.active_tab_pane_group().clone();
            pane_group.update(ctx, |panes, ctx| {
                let notebook_view = ctx.add_typed_action_view(NotebookView::new);
                panes.add_pane_with_direction(
                    Direction::Left,
                    NotebookPane::new(notebook_view, ctx),
                    true,
                    ctx,
                );
            });
        });

        let list_tabs =
            TerminalControlService::handle_for_test(TerminalControlRequest::ListTabs, &mut app);
        let list_panes =
            TerminalControlService::handle_for_test(TerminalControlRequest::ListPanes, &mut app);
        let current_pane =
            TerminalControlService::handle_for_test(TerminalControlRequest::CurrentPane, &mut app);

        match list_tabs {
            TerminalControlResponse::ListTabs(tabs) => assert_eq!(tabs.len(), 2),
            other => panic!("expected tab listing, got {other:?}"),
        }

        match list_panes {
            TerminalControlResponse::ListPanes(panes) => {
                assert_eq!(panes.len(), 3);
                assert!(panes.iter().all(|pane| pane.pane_type == "terminal"));
            }
            other => panic!("expected pane listing, got {other:?}"),
        }

        match current_pane {
            TerminalControlResponse::CurrentPane(pane) => {
                assert!(pane.active);
                assert_eq!(pane.pane_type, "terminal");
            }
            other => panic!("expected current pane, got {other:?}"),
        }
    });
}

#[test]
fn terminal_control_service_active_pane_requires_an_active_window() {
    App::test((), |mut app| async move {
        initialize_app(&mut app);
        let workspace = mock_workspace(&mut app);
        let window_id = app.read(|ctx| workspace.window_id(ctx));

        app.update(|ctx| {
            WindowManager::handle(ctx).update(ctx, |windowing_state: &mut WindowManager, _ctx| {
                windowing_state.overwrite_for_test(ApplicationStage::Active, Some(window_id));
            });
        });

        app.update(|ctx| {
            ctx.windows().hide_window(window_id);
            WindowManager::handle(ctx).update(ctx, |windowing_state: &mut WindowManager, _ctx| {
                windowing_state.overwrite_for_test(ApplicationStage::Inactive, None);
            });
        });

        let current_pane =
            TerminalControlService::handle_for_test(TerminalControlRequest::CurrentPane, &mut app);
        let send_text = TerminalControlService::handle_for_test(
            TerminalControlRequest::SendText {
                target: TerminalControlTarget::ActivePane,
                text: "pwd".to_string(),
            },
            &mut app,
        );

        assert_eq!(
            current_pane,
            TerminalControlResponse::Error(TerminalControlError::NoActiveTerminal)
        );
        assert_eq!(
            send_text,
            TerminalControlResponse::Error(TerminalControlError::NoActiveTerminal)
        );
    });
}

#[test]
fn terminal_control_service_focus_pane_changes_focus_only_when_requested() {
    App::test((), |mut app| async move {
        initialize_app(&mut app);
        let workspace = mock_workspace(&mut app);

        let (initial_pane_id, target_pane_id) = workspace.update(&mut app, |workspace, ctx| {
            let pane_group = workspace.active_tab_pane_group().clone();
            pane_group.update(ctx, |pane_group, ctx| {
                let initial_pane_id = pane_group.pane_id_by_index(0).expect("initial pane exists");
                pane_group.handle_action(&PaneGroupAction::Add(Direction::Right), ctx);
                let target_pane_id = pane_group.pane_id_by_index(1).expect("target pane exists");
                pane_group.focus_pane_by_id(initial_pane_id, ctx);
                (initial_pane_id, target_pane_id)
            })
        });

        let response = TerminalControlService::handle_for_test(
            TerminalControlRequest::FocusPane {
                target: TerminalControlTarget::PaneId(target_pane_id.to_string()),
            },
            &mut app,
        );

        assert_eq!(response, TerminalControlResponse::FocusPane);

        workspace.read(&app, |workspace, ctx| {
            let pane_group = workspace.active_tab_pane_group().as_ref(ctx);
            assert_eq!(pane_group.focused_pane_id(ctx), target_pane_id);
            assert_ne!(pane_group.focused_pane_id(ctx), initial_pane_id);
        });
    });
}

#[test]
fn terminal_control_service_focus_pane_focuses_target_window() {
    App::test((), |mut app| async move {
        initialize_app(&mut app);
        let source_workspace = mock_workspace(&mut app);
        let target_workspace = mock_workspace(&mut app);

        let source_window_id = app.read(|ctx| source_workspace.window_id(ctx));
        let target_window_id = app.read(|ctx| target_workspace.window_id(ctx));
        assert_ne!(source_window_id, target_window_id);

        let target_pane_id = target_workspace.read(&app, |workspace, ctx| {
            workspace
                .active_tab_pane_group()
                .as_ref(ctx)
                .pane_id_by_index(0)
                .expect("target pane exists")
        });

        app.update(|ctx| {
            WindowManager::handle(ctx).update(ctx, |windowing_state: &mut WindowManager, _ctx| {
                windowing_state
                    .overwrite_for_test(ApplicationStage::Active, Some(source_window_id));
            });
        });
        assert_eq!(
            app.read(|ctx| ctx.windows().state().active_window),
            Some(source_window_id)
        );

        let response = TerminalControlService::handle_for_test(
            TerminalControlRequest::FocusPane {
                target: TerminalControlTarget::PaneId(target_pane_id.to_string()),
            },
            &mut app,
        );

        assert_eq!(response, TerminalControlResponse::FocusPane);

        target_workspace.read(&app, |workspace, ctx| {
            let pane_group = workspace.active_tab_pane_group().as_ref(ctx);
            assert_eq!(pane_group.focused_pane_id(ctx), target_pane_id);
        });
    });
}

#[test]
fn terminal_control_service_send_requests_write_bytes_without_changing_focus() {
    App::test((), |mut app| async move {
        initialize_app(&mut app);
        let workspace = mock_workspace(&mut app);

        let (focused_pane_id, target_pane_id, target_terminal) =
            workspace.update(&mut app, |workspace, ctx| {
                let pane_group = workspace.active_tab_pane_group().clone();
                pane_group.update(ctx, |pane_group, ctx| {
                    let focused_pane_id =
                        pane_group.pane_id_by_index(0).expect("focused pane exists");
                    pane_group.handle_action(&PaneGroupAction::Add(Direction::Right), ctx);
                    let target_pane_id =
                        pane_group.pane_id_by_index(1).expect("target pane exists");
                    let target_terminal = pane_group
                        .terminal_view_from_pane_id(target_pane_id, ctx)
                        .expect("target terminal exists");
                    pane_group.focus_pane_by_id(focused_pane_id, ctx);
                    (focused_pane_id, target_pane_id, target_terminal)
                })
            });
        let pty_writes = subscribe_to_pty_writes(&mut app, &target_terminal);

        let send_text = TerminalControlService::handle_for_test(
            TerminalControlRequest::SendText {
                target: TerminalControlTarget::PaneId(target_pane_id.to_string()),
                text: "echo hi".to_string(),
            },
            &mut app,
        );
        let send_key = TerminalControlService::handle_for_test(
            TerminalControlRequest::SendKey {
                target: TerminalControlTarget::PaneId(target_pane_id.to_string()),
                key: "enter".to_string(),
            },
            &mut app,
        );

        assert_eq!(send_text, TerminalControlResponse::SendText);
        assert_eq!(send_key, TerminalControlResponse::SendKey);
        assert_eq!(
            pty_writes.borrow().as_slice(),
            &[b"echo hi".to_vec(), b"\r".to_vec()]
        );

        workspace.read(&app, |workspace, ctx| {
            let pane_group = workspace.active_tab_pane_group().as_ref(ctx);
            assert_eq!(pane_group.focused_pane_id(ctx), focused_pane_id);
        });
    });
}

#[test]
fn terminal_control_service_read_returns_snapshot_for_explicit_pane_without_changing_focus() {
    App::test((), |mut app| async move {
        initialize_app(&mut app);
        let workspace = mock_workspace(&mut app);

        let (focused_pane_id, target_pane_id, target_terminal) =
            workspace.update(&mut app, |workspace, ctx| {
                let pane_group = workspace.active_tab_pane_group().clone();
                pane_group.update(ctx, |pane_group, ctx| {
                    let focused_pane_id =
                        pane_group.pane_id_by_index(0).expect("focused pane exists");
                    pane_group.handle_action(&PaneGroupAction::Add(Direction::Right), ctx);
                    let target_pane_id = pane_group.pane_id_by_index(1).expect("target pane exists");
                    let target_terminal = pane_group
                        .terminal_view_from_pane_id(target_pane_id, ctx)
                        .expect("target terminal exists");
                    pane_group.focus_pane_by_id(focused_pane_id, ctx);
                    (focused_pane_id, target_pane_id, target_terminal)
                })
            });

        target_terminal.update(&mut app, |view, _ctx| {
            let mut model = view.model.lock();
            model.simulate_block("pwd", "/tmp");
        });

        let response = TerminalControlService::handle_for_test(
            TerminalControlRequest::ReadPane {
                target: TerminalControlTarget::PaneId(target_pane_id.to_string()),
            },
            &mut app,
        );

        match response {
            TerminalControlResponse::ReadPane(snapshot) => {
                assert_eq!(snapshot.pane_id, target_pane_id.to_string());
                assert!(!snapshot.focused);
                assert!(!snapshot.active);
                assert_eq!(snapshot.content, "pwd\n/tmp");
                assert!(!snapshot.truncated);
                assert_eq!(snapshot.cursor_row, None);
                assert_eq!(snapshot.cursor_col, None);
            }
            other => panic!("expected pane snapshot, got {other:?}"),
        }

        workspace.read(&app, |workspace, ctx| {
            let pane_group = workspace.active_tab_pane_group().as_ref(ctx);
            assert_eq!(pane_group.focused_pane_id(ctx), focused_pane_id);
        });
    });
}

#[test]
fn terminal_control_service_read_marks_truncated_when_snapshot_is_bounded() {
    App::test((), |mut app| async move {
        initialize_app(&mut app);
        let workspace = mock_workspace(&mut app);

        let (target_pane_id, target_terminal) = workspace.update(&mut app, |workspace, ctx| {
            let pane_group = workspace.active_tab_pane_group().clone();
            pane_group.update(ctx, |pane_group, ctx| {
                let target_pane_id = pane_group.pane_id_by_index(0).expect("target pane exists");
                let target_terminal = pane_group
                    .terminal_view_from_pane_id(target_pane_id, ctx)
                    .expect("target terminal exists");
                (target_pane_id, target_terminal)
            })
        });

        let oversized_output = "0123456789abcdef".repeat(80);

        target_terminal.update(&mut app, |view, _ctx| {
            let mut model = view.model.lock();
            model.simulate_block("printf oversized", &oversized_output);
        });

        let response = TerminalControlService::handle_for_test(
            TerminalControlRequest::ReadPane {
                target: TerminalControlTarget::PaneId(target_pane_id.to_string()),
            },
            &mut app,
        );

        match response {
            TerminalControlResponse::ReadPane(snapshot) => {
                assert_eq!(snapshot.pane_id, target_pane_id.to_string());
                assert!(snapshot.truncated);
                assert!(snapshot.content.ends_with("89abcdef"));
                assert!(snapshot.content.len() < format!("printf oversized\n{oversized_output}").len());
                assert_eq!(snapshot.cursor_row, None);
                assert_eq!(snapshot.cursor_col, None);
            }
            other => panic!("expected pane snapshot, got {other:?}"),
        }
    });
}

#[test]
fn terminal_control_service_send_arrow_key_matches_terminal_input_encoding() {
    App::test((), |mut app| async move {
        initialize_app(&mut app);
        let workspace = mock_workspace(&mut app);

        let (pane_id, terminal) = workspace.update(&mut app, |workspace, ctx| {
            let pane_group = workspace.active_tab_pane_group().clone();
            pane_group.update(ctx, |pane_group, ctx| {
                let pane_id = pane_group.pane_id_by_index(0).expect("pane exists");
                let terminal = pane_group
                    .terminal_view_from_pane_id(pane_id, ctx)
                    .expect("terminal exists");
                (pane_id, terminal)
            })
        });

        let pty_writes = subscribe_to_pty_writes(&mut app, &terminal);
        let expected_up = terminal.read(&app, |terminal, _| {
            let model = terminal.model.lock();
            crate::terminal::model::escape_sequences::EscCodes::build_escape_sequence(
                &*model,
                &[crate::terminal::model::escape_sequences::EscCodes::ARROW_UP],
            )
        });

        let response = TerminalControlService::handle_for_test(
            TerminalControlRequest::SendKey {
                target: TerminalControlTarget::PaneId(pane_id.to_string()),
                key: "up".to_string(),
            },
            &mut app,
        );

        assert_eq!(response, TerminalControlResponse::SendKey);
        assert_eq!(pty_writes.borrow().as_slice(), &[expected_up]);
    });
}

#[test]
fn terminal_control_service_returns_typed_target_errors() {
    App::test((), |mut app| async move {
        initialize_app(&mut app);
        let workspace = mock_workspace(&mut app);

        let notebook_pane_id = workspace.update(&mut app, |workspace, ctx| {
            let pane_group = workspace.active_tab_pane_group().clone();
            pane_group.update(ctx, |panes, ctx| {
                let initial_pane_id = panes.pane_id_by_index(0).expect("initial pane exists");
                let notebook_view = ctx.add_typed_action_view(NotebookView::new);
                panes.add_pane_with_direction(
                    Direction::Left,
                    NotebookPane::new(notebook_view, ctx),
                    true,
                    ctx,
                );
                panes
                    .pane_ids()
                    .find(|pane_id| *pane_id != initial_pane_id)
                    .expect("notebook pane exists")
            })
        });

        let not_found = TerminalControlService::handle_for_test(
            TerminalControlRequest::FocusPane {
                target: TerminalControlTarget::PaneId("missing-pane".to_string()),
            },
            &mut app,
        );
        let non_terminal = TerminalControlService::handle_for_test(
            TerminalControlRequest::SendText {
                target: TerminalControlTarget::PaneId(notebook_pane_id.to_string()),
                text: "echo nope".to_string(),
            },
            &mut app,
        );

        assert_eq!(
            not_found,
            TerminalControlResponse::Error(TerminalControlError::PaneNotFound {
                pane_id: Some("missing-pane".to_string()),
            })
        );
        assert_eq!(
            non_terminal,
            TerminalControlResponse::Error(TerminalControlError::PaneNotTerminal {
                pane_id: notebook_pane_id.to_string(),
            })
        );
    });
}

#[test]
fn terminal_control_service_returns_non_writable_and_unsupported_key_errors() {
    App::test((), |mut app| async move {
        initialize_app(&mut app);
        let workspace = mock_workspace_viewing_shared_session(&mut app);
        let writable_workspace = mock_workspace(&mut app);
        let window_id = app.read(|ctx| workspace.window_id(ctx));

        app.update(|ctx| {
            WindowManager::handle(ctx).update(ctx, |windowing_state: &mut WindowManager, _ctx| {
                windowing_state.overwrite_for_test(ApplicationStage::Active, Some(window_id));
            });
        });

        let pane_id = workspace.read(&app, |workspace, ctx| {
            workspace
                .active_tab_pane_group()
                .as_ref(ctx)
                .pane_id_by_index(0)
                .expect("viewer pane exists")
        });
        let writable_pane_id = writable_workspace.read(&app, |workspace, ctx| {
            workspace
                .active_tab_pane_group()
                .as_ref(ctx)
                .pane_id_by_index(0)
                .expect("writable pane exists")
        });

        let not_writable = TerminalControlService::handle_for_test(
            TerminalControlRequest::SendText {
                target: TerminalControlTarget::PaneId(pane_id.to_string()),
                text: "echo nope".to_string(),
            },
            &mut app,
        );
        let unsupported_key = TerminalControlService::handle_for_test(
            TerminalControlRequest::SendKey {
                target: TerminalControlTarget::PaneId(writable_pane_id.to_string()),
                key: "f13".to_string(),
            },
            &mut app,
        );

        assert_eq!(
            not_writable,
            TerminalControlResponse::Error(TerminalControlError::PaneNotWritable {
                pane_id: pane_id.to_string(),
            })
        );
        assert_eq!(
            unsupported_key,
            TerminalControlResponse::Error(TerminalControlError::UnsupportedKey {
                key: "f13".to_string(),
            })
        );
    });
}
