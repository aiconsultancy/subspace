use serde::Deserialize;

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct Environment {
    pub id: u64,
    pub name: String,
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub html_url: String,
    #[serde(default)]
    pub deployment_branch_policy: Option<DeploymentBranchPolicy>,
    #[serde(default)]
    pub protection_rules: Vec<ProtectionRule>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct DeploymentBranchPolicy {
    pub protected_branches: bool,
    pub custom_branch_policies: bool,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct ProtectionRule {
    pub id: u64,
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default)]
    pub wait_timer: Option<u64>,
    #[serde(default)]
    pub reviewers: Vec<Reviewer>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct ReviewerIdentity {
    #[serde(default)]
    pub login: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub slug: Option<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct Reviewer {
    #[serde(rename = "type", default)]
    pub kind: String,
    #[serde(default)]
    pub reviewer: Option<ReviewerIdentity>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct PendingDeployment {
    pub environment: Environment,
    #[serde(default)]
    pub wait_timer: u64,
    #[serde(default)]
    pub wait_timer_started_at: Option<String>,
    #[serde(default)]
    pub reviewers: Vec<Reviewer>,
    #[serde(default)]
    pub current_user_can_approve: bool,
}
