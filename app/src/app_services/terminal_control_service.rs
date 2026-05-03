use crate::pane_group::PaneId;
use crate::workspace::WorkspaceRegistry;
use async_trait::async_trait;
use futures::channel::oneshot;
use warp_core::channel::ChannelState;
#[cfg(test)]
use warpui::App;
use warpui::{AppContext, SingletonEntity, WindowId};
use warpui::{Entity, ModelContext};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum TerminalControlTarget {
    ActivePane,
    PaneId(String),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TabSummary {
    pub tab_id: String,
    pub title: String,
    pub active: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PaneSummary {
    pub pane_id: String,
    pub tab_id: String,
    pub workspace_id: Option<String>,
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
    FocusPane {
        target: TerminalControlTarget,
    },
    SendText {
        target: TerminalControlTarget,
        text: String,
    },
    SendKey {
        target: TerminalControlTarget,
        key: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum TerminalControlError {
    PaneNotFound { pane_id: Option<String> },
    PaneNotTerminal { pane_id: String },
    PaneNotWritable { pane_id: String },
    NoActiveTerminal,
    UnsupportedKey { key: String },
    IpcUnavailable,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum TerminalControlResponse {
    ListTabs(Vec<TabSummary>),
    ListPanes(Vec<PaneSummary>),
    CurrentPane(PaneSummary),
    FocusPane,
    SendText,
    SendKey,
    Error(TerminalControlError),
}

pub struct TerminalControlService;

impl ipc::Service for TerminalControlService {
    type Request = TerminalControlRequest;
    type Response = TerminalControlResponse;
}

#[derive(Clone, Default)]
pub(crate) struct TerminalControlServiceImpl {
    request_tx: Option<
        async_channel::Sender<(
            TerminalControlRequest,
            oneshot::Sender<TerminalControlResponse>,
        )>,
    >,
}

impl TerminalControlService {
    #[cfg(test)]
    pub(crate) fn handle_for_test(
        request: TerminalControlRequest,
        app: &mut App,
    ) -> TerminalControlResponse {
        app.update(|ctx| TerminalControlServiceImpl::default().handle_request_sync(request, ctx))
    }
}

pub(crate) fn terminal_control_service_address() -> String {
    format!("Warp{:?}_TERMINAL_CONTROL", ChannelState::channel())
}

pub(crate) struct TerminalControlServiceHost {
    _server: Option<ipc::Server>,
}

impl TerminalControlServiceHost {
    pub(crate) fn new(ctx: &mut ModelContext<Self>) -> Self {
        let (request_tx, request_rx) = async_channel::unbounded::<(
            TerminalControlRequest,
            oneshot::Sender<TerminalControlResponse>,
        )>();
        let server = match ipc::ServerBuilder::default()
            .with_fixed_address(terminal_control_service_address())
            .with_service(TerminalControlServiceImpl::new(request_tx))
            .build_and_run(ctx.background_executor())
        {
            Ok((server, _)) => Some(server),
            Err(err) => {
                log::error!("Failed to initialize TerminalControlService server: {err:?}");
                None
            }
        };

        ctx.spawn_stream_local(
            request_rx,
            |_, (request, response_tx), ctx| {
                let response =
                    TerminalControlServiceImpl::default().handle_request_sync(request, ctx);
                let _ = response_tx.send(response);
            },
            |_, _| {},
        );

        Self { _server: server }
    }
}

impl Entity for TerminalControlServiceHost {
    type Event = ();
}

impl SingletonEntity for TerminalControlServiceHost {}

#[async_trait]
impl ipc::ServiceImpl for TerminalControlServiceImpl {
    type Service = TerminalControlService;

    async fn handle_request(&self, request: TerminalControlRequest) -> TerminalControlResponse {
        let Some(request_tx) = &self.request_tx else {
            return TerminalControlResponse::Error(TerminalControlError::IpcUnavailable);
        };

        let (response_tx, response_rx) = oneshot::channel();
        if request_tx.send((request, response_tx)).await.is_err() {
            return TerminalControlResponse::Error(TerminalControlError::IpcUnavailable);
        }

        response_rx.await.unwrap_or(TerminalControlResponse::Error(
            TerminalControlError::IpcUnavailable,
        ))
    }
}

impl TerminalControlServiceImpl {
    pub(crate) fn new(
        request_tx: async_channel::Sender<(
            TerminalControlRequest,
            oneshot::Sender<TerminalControlResponse>,
        )>,
    ) -> Self {
        Self {
            request_tx: Some(request_tx),
        }
    }

    pub(crate) fn handle_request_sync(
        &self,
        request: TerminalControlRequest,
        ctx: &mut AppContext,
    ) -> TerminalControlResponse {
        match request {
            TerminalControlRequest::ListTabs => TerminalControlResponse::ListTabs(list_tabs(ctx)),
            TerminalControlRequest::ListPanes => {
                TerminalControlResponse::ListPanes(list_panes(ctx))
            }
            TerminalControlRequest::CurrentPane => current_pane(ctx)
                .map(TerminalControlResponse::CurrentPane)
                .unwrap_or(TerminalControlResponse::Error(
                    TerminalControlError::NoActiveTerminal,
                )),
            TerminalControlRequest::FocusPane { target } => {
                match resolve_terminal_target(&target, ctx) {
                    Ok(resolved) => {
                        focus_terminal_target(&resolved, ctx);
                        TerminalControlResponse::FocusPane
                    }
                    Err(error) => TerminalControlResponse::Error(error),
                }
            }
            TerminalControlRequest::SendText { target, text } => {
                match resolve_writable_terminal(&target, ctx) {
                    Ok(resolved) => {
                        resolved.terminal.update(ctx, |terminal, ctx| {
                            terminal.write_programmatic_bytes_to_pty(text.into_bytes(), ctx);
                        });
                        TerminalControlResponse::SendText
                    }
                    Err(error) => TerminalControlResponse::Error(error),
                }
            }
            TerminalControlRequest::SendKey { target, key } => {
                match resolve_writable_terminal(&target, ctx) {
                    Ok(resolved) => {
                        let Some(bytes) = resolved
                            .terminal
                            .read(ctx, |terminal, _| terminal.encode_programmatic_key(&key))
                        else {
                            return TerminalControlResponse::Error(
                                TerminalControlError::UnsupportedKey { key },
                            );
                        };

                        resolved.terminal.update(ctx, |terminal, ctx| {
                            terminal.write_programmatic_bytes_to_pty(bytes, ctx);
                        });
                        TerminalControlResponse::SendKey
                    }
                    Err(error) => TerminalControlResponse::Error(error),
                }
            }
        }
    }
}

#[derive(Clone)]
struct ResolvedTarget {
    workspace: warpui::ViewHandle<crate::workspace::Workspace>,
    locator: crate::workspace::PaneViewLocator,
    terminal: warpui::ViewHandle<crate::terminal::TerminalView>,
}

fn list_tabs(ctx: &AppContext) -> Vec<TabSummary> {
    let active_window = ctx.windows().state().active_window;

    WorkspaceRegistry::as_ref(ctx)
        .all_workspaces(ctx)
        .into_iter()
        .flat_map(
            |(window_id, workspace): (
                WindowId,
                warpui::ViewHandle<crate::workspace::Workspace>,
            )| {
                let is_active_window = Some(window_id) == active_window;
                workspace.read(ctx, |workspace: &crate::workspace::Workspace, ctx| {
                    workspace
                        .tab_views()
                        .enumerate()
                        .map(|(tab_index, pane_group)| TabSummary {
                            tab_id: pane_group.id().to_string(),
                            title: pane_group.as_ref(ctx).display_title(ctx),
                            active: is_active_window && workspace.active_tab_index() == tab_index,
                        })
                        .collect::<Vec<_>>()
                })
            },
        )
        .collect()
}

fn list_panes(ctx: &AppContext) -> Vec<PaneSummary> {
    let active_window = ctx.windows().state().active_window;

    WorkspaceRegistry::as_ref(ctx)
        .all_workspaces(ctx)
        .into_iter()
        .flat_map(
            |(window_id, workspace): (
                WindowId,
                warpui::ViewHandle<crate::workspace::Workspace>,
            )| {
                let is_active_window = Some(window_id) == active_window;
                workspace.read(ctx, |workspace: &crate::workspace::Workspace, ctx| {
                    workspace
                        .tab_views()
                        .enumerate()
                        .flat_map(|(tab_index, pane_group)| {
                            let tab_id = pane_group.id().to_string();
                            let pane_group = pane_group.as_ref(ctx);
                            let active_pane_id = pane_group.active_session_id(ctx).map(Into::into);
                            let focused_pane_id = pane_group.focused_pane_id(ctx);

                            pane_group
                                .terminal_pane_ids()
                                .map(|pane_id| {
                                    build_pane_summary(
                                        pane_id,
                                        &tab_id,
                                        window_id,
                                        is_active_window
                                            && workspace.active_tab_index() == tab_index,
                                        focused_pane_id,
                                        active_pane_id,
                                        &pane_group,
                                        ctx,
                                    )
                                })
                                .collect::<Vec<_>>()
                        })
                        .collect::<Vec<_>>()
                })
            },
        )
        .collect()
}

fn current_pane(ctx: &AppContext) -> Option<PaneSummary> {
    let workspace = active_workspace(ctx)?;
    let active_window = workspace.window_id(ctx);

    workspace.read(ctx, |workspace: &crate::workspace::Workspace, ctx| {
        let pane_group = workspace.active_tab_pane_group().as_ref(ctx);
        let pane_id = if pane_group.focused_session_view(ctx).is_some() {
            pane_group.focused_pane_id(ctx)
        } else {
            pane_group.active_session_id(ctx)?.into()
        };

        Some(build_pane_summary(
            pane_id,
            &workspace.active_tab_pane_group().id().to_string(),
            active_window,
            true,
            pane_group.focused_pane_id(ctx),
            Some(pane_id),
            pane_group,
            ctx,
        ))
    })
}

fn active_workspace(ctx: &AppContext) -> Option<warpui::ViewHandle<crate::workspace::Workspace>> {
    let active_window = ctx.windows().state().active_window?;
    WorkspaceRegistry::as_ref(ctx).get(active_window, ctx)
}

fn build_pane_summary(
    pane_id: PaneId,
    tab_id: &str,
    window_id: warpui::WindowId,
    tab_is_active: bool,
    focused_pane_id: PaneId,
    active_pane_id: Option<PaneId>,
    pane_group: &crate::pane_group::PaneGroup,
    ctx: &AppContext,
) -> PaneSummary {
    let terminal = pane_group
        .terminal_view_from_pane_id(pane_id, ctx)
        .expect("pane summary should only be built for terminal panes");
    let (title, cwd, command) = terminal.read(ctx, |terminal, ctx| {
        let title = terminal
            .model
            .lock()
            .terminal_title()
            .unwrap_or_else(|| pane_group.title(ctx));
        let cwd = terminal.pwd_if_local(ctx);
        let command = None;
        (title, cwd, command)
    });

    PaneSummary {
        pane_id: pane_id.to_string(),
        tab_id: tab_id.to_string(),
        workspace_id: Some(window_id.to_string()),
        title,
        cwd,
        command,
        focused: pane_id == focused_pane_id,
        active: tab_is_active && active_pane_id == Some(pane_id),
        pane_type: "terminal".to_string(),
    }
}

fn focus_terminal_target(resolved: &ResolvedTarget, ctx: &mut AppContext) {
    let target_window_id = resolved.workspace.window_id(ctx);
    if target_window_id
        != ctx
            .windows()
            .state()
            .active_window
            .unwrap_or(target_window_id)
    {
        ctx.windows().show_window_and_focus_app(target_window_id);
    }

    resolved.workspace.update(ctx, |workspace, ctx| {
        workspace.focus_pane(resolved.locator, ctx)
    });
}

fn resolve_terminal_target(
    target: &TerminalControlTarget,
    ctx: &AppContext,
) -> Result<ResolvedTarget, TerminalControlError> {
    match target {
        TerminalControlTarget::ActivePane => {
            let workspace = active_workspace(ctx).ok_or(TerminalControlError::NoActiveTerminal)?;

            let (locator, terminal) =
                workspace.read(ctx, |workspace: &crate::workspace::Workspace, ctx| {
                    let pane_group = workspace.active_tab_pane_group().clone();
                    let pane_group_ref = pane_group.as_ref(ctx);
                    let (pane_id, terminal) =
                        if let Some(terminal) = pane_group_ref.focused_session_view(ctx) {
                            (pane_group_ref.focused_pane_id(ctx), terminal)
                        } else {
                            let pane_id = pane_group_ref
                                .active_session_id(ctx)
                                .map(Into::into)
                                .ok_or(TerminalControlError::NoActiveTerminal)?;
                            let terminal = pane_group_ref
                                .terminal_view_from_pane_id(pane_id, ctx)
                                .ok_or(TerminalControlError::NoActiveTerminal)?;
                            (pane_id, terminal)
                        };

                    Ok((
                        crate::workspace::PaneViewLocator {
                            pane_group_id: pane_group.id(),
                            pane_id,
                        },
                        terminal,
                    ))
                })?;

            Ok(ResolvedTarget {
                workspace,
                locator,
                terminal,
            })
        }
        TerminalControlTarget::PaneId(raw_pane_id) => {
            let all_workspaces = WorkspaceRegistry::as_ref(ctx).all_workspaces(ctx);
            for (_, workspace) in all_workspaces {
                let maybe_target: Option<(
                    crate::workspace::PaneViewLocator,
                    Option<warpui::ViewHandle<crate::terminal::TerminalView>>,
                )> = workspace.read(ctx, |workspace: &crate::workspace::Workspace, ctx| {
                    workspace.tab_views().find_map(|pane_group| {
                        let pane_group_ref = pane_group.as_ref(ctx);
                        pane_group_ref.pane_ids().find_map(|pane_id: PaneId| {
                            if pane_id.to_string() != raw_pane_id.as_str() {
                                return None;
                            }

                            let terminal = pane_group_ref.terminal_view_from_pane_id(pane_id, ctx);
                            Some((
                                crate::workspace::PaneViewLocator {
                                    pane_group_id: pane_group.id(),
                                    pane_id,
                                },
                                terminal,
                            ))
                        })
                    })
                });

                if let Some((locator, terminal)) = maybe_target {
                    let terminal =
                        terminal.ok_or_else(|| TerminalControlError::PaneNotTerminal {
                            pane_id: raw_pane_id.clone(),
                        })?;
                    return Ok(ResolvedTarget {
                        workspace,
                        locator,
                        terminal,
                    });
                }
            }

            Err(TerminalControlError::PaneNotFound {
                pane_id: Some(raw_pane_id.clone()),
            })
        }
    }
}

fn resolve_writable_terminal(
    target: &TerminalControlTarget,
    ctx: &AppContext,
) -> Result<ResolvedTarget, TerminalControlError> {
    let resolved = resolve_terminal_target(target, ctx)?;
    let pane_id = resolved.locator.pane_id.to_string();

    let is_writable = resolved.terminal.read(ctx, |terminal, _| {
        let model = terminal.model.lock();
        !model.shared_session_status().is_viewer()
    });

    if is_writable {
        Ok(resolved)
    } else {
        Err(TerminalControlError::PaneNotWritable { pane_id })
    }
}

#[cfg(test)]
#[path = "terminal_control_service_tests.rs"]
mod tests;
