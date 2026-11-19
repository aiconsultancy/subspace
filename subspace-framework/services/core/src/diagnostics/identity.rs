/// Registered build and platform identities shared by both diagnostics profiles.
/// Values must be product/build identifiers, never user-configured names.
#[derive(Debug, Clone)]
pub struct BuildIdentity {
    product_id: String,
    app_version: String,
    channel: Option<String>,
    commit: Option<String>,
    os: Option<String>,
    architecture: Option<String>,
}

impl BuildIdentity {
    pub fn new(product_id: impl Into<String>, app_version: impl Into<String>) -> Self {
        Self {
            product_id: product_id.into(),
            app_version: app_version.into(),
            channel: None,
            commit: None,
            os: None,
            architecture: None,
        }
    }

    pub fn with_build(mut self, channel: impl Into<String>, commit: impl Into<String>) -> Self {
        self.channel = Some(channel.into());
        self.commit = Some(commit.into());
        self
    }

    pub fn with_platform(mut self, os: impl Into<String>, architecture: impl Into<String>) -> Self {
        self.os = Some(os.into());
        self.architecture = Some(architecture.into());
        self
    }

    pub fn product_id(&self) -> &str {
        &self.product_id
    }

    pub fn app_version(&self) -> &str {
        &self.app_version
    }

    pub fn channel(&self) -> Option<&str> {
        self.channel.as_deref()
    }

    pub fn commit(&self) -> Option<&str> {
        self.commit.as_deref()
    }

    pub fn os(&self) -> Option<&str> {
        self.os.as_deref()
    }

    pub fn architecture(&self) -> Option<&str> {
        self.architecture.as_deref()
    }
}
