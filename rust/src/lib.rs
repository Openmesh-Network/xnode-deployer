use std::net::Ipv4Addr;

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

pub enum OptionalSupport<T> {
    NotSupported,
    Supported(T),
}

pub trait XnodeDeployer: Send + Sync {
    type ProviderOutput;

    /// Provision new hardware with XnodeOS
    fn deploy(
        &self,
        input: DeployInput,
    ) -> impl Future<Output = Result<Self::ProviderOutput, Error>> + Send;

    /// Cancel renting of hardware
    fn undeploy(&self, xnode: Self::ProviderOutput) -> impl Future<Output = Option<Error>> + Send;

    /// Get ipv4 address of deployed hardware
    fn ipv4(
        &self,
        xnode: Self::ProviderOutput,
    ) -> impl Future<Output = Result<OptionalSupport<Option<Ipv4Addr>>, Error>> + Send;
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
