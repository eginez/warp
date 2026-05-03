use clap::Args;

#[derive(Debug, Clone, clap::Subcommand)]
pub enum TerminalControlCommand {
    #[command(name = "create-tab")]
    /// Create a new terminal tab and return its initial pane handle.
    CreateTab,
    #[command(name = "list-tabs")]
    /// List terminal tabs.
    ListTabs,
    #[command(name = "list-panes")]
    /// List terminal panes and their pane handles.
    ListPanes,
    #[command(name = "current-pane")]
    /// Show the current terminal pane and its pane handle.
    CurrentPane,
    #[command(name = "focus-pane")]
    /// Focus a terminal pane by pane handle.
    FocusPane(PaneTargetArgs),
    #[command(name = "read")]
    /// Read a bounded text snapshot from the current pane.
    Read,
    #[command(name = "read-pane")]
    /// Read a bounded text snapshot from a terminal pane by pane handle without changing focus.
    ReadPane(PaneTargetArgs),
    #[command(name = "send")]
    /// Send text to the current pane without changing focus.
    Send(SendTextArgs),
    #[command(name = "send-pane")]
    /// Send text to a terminal pane by pane handle without changing focus.
    SendPane(SendTextToPaneArgs),
    #[command(name = "send-key")]
    /// Send a key to the current pane without changing focus.
    SendKey(SendKeyArgs),
    #[command(name = "send-key-pane")]
    /// Send a key to a terminal pane by pane handle without changing focus.
    SendKeyPane(SendKeyToPaneArgs),
}

#[derive(Debug, Clone, Args)]
pub struct PaneTargetArgs {
    /// The target pane handle.
    pub pane_id: String,
}

#[derive(Debug, Clone, Args)]
pub struct SendTextArgs {
    /// The text to send.
    pub text: String,
}

#[derive(Debug, Clone, Args)]
pub struct SendTextToPaneArgs {
    /// The target pane handle.
    pub pane_id: String,
    /// The text to send.
    pub text: String,
}

#[derive(Debug, Clone, Args)]
pub struct SendKeyArgs {
    /// The key to send.
    pub key: String,
}

#[derive(Debug, Clone, Args)]
pub struct SendKeyToPaneArgs {
    /// The target pane handle.
    pub pane_id: String,
    /// The key to send.
    pub key: String,
}
