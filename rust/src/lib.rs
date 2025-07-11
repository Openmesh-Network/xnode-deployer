use serde::{Deserialize, Serialize};

mod utils;
pub use utils::{Error, XnodeDeployerError};

#[cfg(feature = "hivelocity")]
pub mod hivelocity;

#[derive(Serialize, Deserialize, Debug)]
pub struct DeployInput {
    pub xnode_owner: Option<String>,
    pub domain: Option<String>,
    pub acme_email: Option<String>,
    pub user_passwd: Option<String>,
    pub encrypted: Option<String>,
    pub initial_config: Option<String>,
}
#[derive(Serialize, Deserialize, Debug)]
pub struct DeployOutput<ProviderOutput> {
    pub ip: String,
    pub provider: ProviderOutput,
}

pub trait XnodeDeployer: Send + Sync {
    type ProviderOutput;

    /// Provision new hardware with XnodeOS
    fn deploy(
        &self,
        input: DeployInput,
    ) -> impl Future<Output = Result<DeployOutput<Self::ProviderOutput>, Error>> + Send;

    /// Cancel renting of hardware
    fn undeploy(
        &self,
        xnode: DeployOutput<Self::ProviderOutput>,
    ) -> impl Future<Output = Option<Error>> + Send;
}

impl DeployInput {
    pub fn cloud_init(&self) -> String {
        let mut env = vec![];
        for (name, content) in [
            ("XNODE_OWNER", &self.xnode_owner),
            ("DOMAIN", &self.domain),
            ("ACME_EMAIL", &self.acme_email),
            ("USER_PASSWD", &self.user_passwd),
            ("ENCRYPTED", &self.encrypted),
            ("INITIAL_CONFIG", &self.initial_config),
        ] {
            if let Some(content) = content {
                env.push(format!("export {name}=\"{content}\" && "));
            }
        }

        let env = env.join("");
        format!(
            "#cloud-config\nruncmd:\n - {env} curl https://raw.githubusercontent.com/Openmesh-Network/xnode-manager/main/os/install.sh | bash 2>&1 | tee /tmp/xnodeos.log"
        )
    }
}
