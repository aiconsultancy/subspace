//! The settings dialog's wording, kept out of the element builders.

/// Field and button labels.
pub(super) mod text {
    pub(in crate::settings) const CONFIG_FILE: &str = "Configuration file";
    pub(in crate::settings) const ENABLED: &str = "Enabled";
    pub(in crate::settings) const CHAT_ENGINE: &str = "Chat engine";
    pub(in crate::settings) const ACP_AGENT: &str = "Agent";
    pub(in crate::settings) const PICK_A_DASHBOARD: &str =
        "Choose a Space on the left to edit its connection settings.";
    pub(in crate::settings) const BOARD_OFF: &str = "Turn on this Space to change its settings.";
    pub(in crate::settings) const PICK_ACCOUNT: &str = "Choose an account";
    pub(in crate::settings) const NO_ACCOUNTS: &str = "No GitHub accounts yet";
    pub(in crate::settings) const GITHUB_ACCOUNTS: &str = "GitHub accounts";
    pub(in crate::settings) const ACCOUNT: &str = "Account";
    pub(in crate::settings) const REPOS_LABEL: &str = "Favourite repositories";
}

/// The small print under the controls.
pub(super) mod help {
    pub(in crate::settings) const CONFIRM_QUIT: &str =
        "Ask for confirmation before quitting the app.";
    pub(in crate::settings) const PRIORITY: &str = "Environment variables take priority over this file. GitHub can fall back to a signed-in account or the gh CLI login. Blank token fields keep saved tokens; Clear removes only the file token.";
    pub(in crate::settings) const RESTART: &str = "Service changes take effect after restarting Subspace. Space choices apply on Save. Tokens typed here are stored in the private config file; signed-in GitHub accounts keep their tokens in the system keychain.";
    pub(in crate::settings) const CHAT_ENGINE: &str = "Choose an engine to turn on chat. Each engine uses its own sign-in. Save and restart Subspace to apply the change.";
    pub(in crate::settings) const ACP_AGENT: &str = "Subspace starts the agent's ACP server and offers it this app's board tools over MCP. The agent keeps its own sign-in, tools and sandbox; its permission requests appear in chat for approval. Set chat.acp.command in the config file for a custom agent.";
    pub(in crate::settings) const GH_ACCOUNT: &str = "After Save, update checks and web Space validation use this account. Restart Subspace to apply the account to other GitHub Spaces and Copilot chat.";
    pub(in crate::settings) const GH_ACCOUNTS: &str = "Sign in and Sign out apply at once, without Save. Subspace keeps each sign-in in the system keychain. Accounts from the GitHub CLI (gh) are listed too; gh manages them.";
    /// For an app without Sign in with GitHub (no OAuth App).
    pub(in crate::settings) const GH_CLI_ACCOUNTS: &str =
        "Accounts from the GitHub CLI (gh) are listed here; gh manages them.";
    pub(in crate::settings) const GH_NO_ACCOUNTS: &str = "No accounts yet. Sign in with GitHub, or run gh auth login in a terminal and reopen Settings.";
    /// For an app without Sign in with GitHub (no OAuth App).
    pub(in crate::settings) const GH_NO_CLI_ACCOUNTS: &str =
        "No accounts yet. Run gh auth login in a terminal and reopen Settings.";
    pub(in crate::settings) const GH_TOKEN: &str = "GH_TOKEN and GITHUB_TOKEN take priority, then this token. With no token, Subspace uses the chosen account. Token changes need Save and a restart.";
    pub(in crate::settings) const APPEARANCE: &str = "Theme and font changes preview immediately. Save keeps them for the next launch; Cancel restores the previous appearance. Cupertino Dark is the framework default; saved theme choices still take priority.";
    pub(in crate::settings) const THEME_FILES: &str = "Add GPUI Kit theme JSON files to";
}
