use clap::Args;

#[derive(Debug, Clone, clap::Subcommand)]
pub enum TerminalControlCommand {
    #[command(name = "list-tabs")]
    /// List terminal tabs.
    ListTabs,
    #[command(name = "list-panes")]
    /// List terminal panes.
    ListPanes,
    #[command(name = "current-pane")]
    /// Show the current terminal pane.
    CurrentPane,
    #[command(name = "focus-pane")]
    /// Focus a terminal pane.
    FocusPane(PaneTargetArgs),
    #[command(name = "read")]
    /// Read text from the current pane.
    Read,
    #[command(name = "read-pane")]
    /// Read text from a terminal pane.
    ReadPane(PaneTargetArgs),
    #[command(name = "send")]
    /// Send text to the current pane.
    Send(SendTextArgs),
    #[command(name = "send-pane")]
    /// Send text to a terminal pane.
    SendPane(SendTextToPaneArgs),
    #[command(name = "send-key")]
    /// Send a key to the current pane.
    SendKey(SendKeyArgs),
    #[command(name = "send-key-pane")]
    /// Send a key to a terminal pane.
    SendKeyPane(SendKeyToPaneArgs),
}

#[derive(Debug, Clone, Args)]
pub struct PaneTargetArgs {
    /// The target pane ID.
    pub pane_id: String,
}

#[derive(Debug, Clone, Args)]
pub struct SendTextArgs {
    /// The text to send.
    pub text: String,
}

#[derive(Debug, Clone, Args)]
pub struct SendTextToPaneArgs {
    /// The target pane ID.
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
    /// The target pane ID.
    pub pane_id: String,
    /// The key to send.
    pub key: String,
}
