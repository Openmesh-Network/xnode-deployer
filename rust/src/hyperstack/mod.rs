use std::{fmt::Display, net::Ipv4Addr, str::FromStr};

use reqwest::Client;
use serde_json::json;

use crate::{
    DeployInput, Error,
    OptionalSupport::{self, Supported},
    XnodeDeployer, XnodeDeployerError,
    utils::XnodeDeployerErrorInner,
};

#[derive(Debug)]
pub enum HyperstackError {
    ResponseNotObject {
        response: serde_json::Value,
    },
    ResponseMissingId {
        map: serde_json::Map<String, serde_json::Value>,
    },
    ResponseMissingInstances {
        map: serde_json::Map<String, serde_json::Value>,
    },
    ResponseInvalidInstances {
        instances: serde_json::Value,
    },
    ResponseEmptyInstances {},
    ResponseInvalidId {
        id: serde_json::Value,
    },
}

impl Display for HyperstackError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(
            match self {
                HyperstackError::ResponseNotObject { response } => {
                    format!("Hyperstack response not object: {response}")
                }
                HyperstackError::ResponseMissingInstances { map } => {
                    format!("Hyperstack response missing instances: {map:?}")
                }
                HyperstackError::ResponseInvalidInstances { instances } => {
                    format!("Hyperstack response invalid instances: {instances:?}")
                }
                HyperstackError::ResponseEmptyInstances {} => {
                    format!("Hyperstack response empty instances")
                }
                HyperstackError::ResponseMissingId { map } => {
                    format!("Hyperstack response missing id: {map:?}")
                }
                HyperstackError::ResponseInvalidId { id } => {
                    format!("Hyperstack response invalid id: {id}")
                }
            }
            .as_str(),
        )
    }
}

pub struct HyperstackDeployer {
    client: Client,
    api_key: String,
    hardware: HyperstackHardware,
}

impl HyperstackDeployer {
    pub fn new(api_key: String, hardware: HyperstackHardware) -> Self {
        Self {
            client: Client::new(),
            api_key,
            hardware,
        }
    }
}

impl XnodeDeployer for HyperstackDeployer {
    type ProviderOutput = HyperstackOutput;

    async fn deploy(&self, input: DeployInput) -> Result<Self::ProviderOutput, Error> {
        log::info!(
            "Hyperstack deployment of {input:?} on {hardware:?} started",
            hardware = self.hardware
        );
        let response = match &self.hardware {
            HyperstackHardware::VirtualMachine {
                name,
                environment_name,
                flavor_name,
                key_name,
            } => self
                .client
                .post("https://infrahub-api.nexgencloud.com/v1/core/virtual-machines")
                .json(&json!({
                    "name": name,
                    "environment_name": environment_name,
                    "image_name": "Ubuntu Server 22.04 LTS (Jammy Jellyfish)",
                    "flavor_name": flavor_name,
                    "key_name": key_name,
                    "count": 1,
                    "assign_floating_ip": true,
                    "user_data": input.cloud_init(),
                    "security_rules": [
                        {
                            "direction": "ingress",
                            "protocol": "tcp",
                            "ethertype": "IPv4",
                            "remote_ip_prefix": "0.0.0.0/0",
                            "port_range_min": 1,
                            "port_range_max": 65535
                        },
                        {
                            "direction": "ingress",
                            "protocol": "udp",
                            "ethertype": "IPv4",
                            "remote_ip_prefix": "0.0.0.0/0",
                            "port_range_min": 1,
                            "port_range_max": 65535
                        }
                    ]
                })),
        }
        .header("api_key", self.api_key.clone())
        .send()
        .await
        .and_then(|response| response.error_for_status())
        .map_err(Error::ReqwestError)?
        .json::<serde_json::Value>()
        .await
        .map_err(Error::ReqwestError)?;

        let id = match &response {
            serde_json::Value::Object(map) => map
                .get("instances")
                .ok_or(Error::XnodeDeployerError(XnodeDeployerError::new(
                    XnodeDeployerErrorInner::HyperstackError(
                        HyperstackError::ResponseMissingInstances { map: map.clone() },
                    ),
                )))
                .and_then(|instances| match instances {
                    serde_json::Value::Array(array) => {
                        array
                            .first()
                            .ok_or(Error::XnodeDeployerError(XnodeDeployerError::new(
                                XnodeDeployerErrorInner::HyperstackError(
                                    HyperstackError::ResponseEmptyInstances {},
                                ),
                            )))
                    }
                    _ => Err(Error::XnodeDeployerError(XnodeDeployerError::new(
                        XnodeDeployerErrorInner::HyperstackError(
                            HyperstackError::ResponseInvalidInstances {
                                instances: instances.clone(),
                            },
                        ),
                    ))),
                })
                .and_then(|instance| match instance {
                    serde_json::Value::Object(map) => map
                        .get("id")
                        .ok_or(Error::XnodeDeployerError(XnodeDeployerError::new(
                            XnodeDeployerErrorInner::HyperstackError(
                                HyperstackError::ResponseMissingId { map: map.clone() },
                            ),
                        )))
                        .and_then(|id| {
                            match id {
                                serde_json::Value::Number(number) => number.as_u64(),
                                _ => None,
                            }
                            .ok_or(Error::XnodeDeployerError(
                                XnodeDeployerError::new(XnodeDeployerErrorInner::HyperstackError(
                                    HyperstackError::ResponseInvalidId { id: id.clone() },
                                )),
                            ))
                        }),
                    _ => Err(Error::XnodeDeployerError(XnodeDeployerError::new(
                        XnodeDeployerErrorInner::HyperstackError(
                            HyperstackError::ResponseNotObject {
                                response: response.clone(),
                            },
                        ),
                    ))),
                }),
            _ => Err(Error::XnodeDeployerError(XnodeDeployerError::new(
                XnodeDeployerErrorInner::HyperstackError(HyperstackError::ResponseNotObject {
                    response: response.clone(),
                }),
            ))),
        };
        let id = match id {
            Ok(id) => id,
            Err(e) => return Err(e),
        };

        let output = Self::ProviderOutput { id };
        log::info!("Hyperstack deployment succeeded: {output:?}");
        Ok(output)
    }

    async fn undeploy(&self, xnode: Self::ProviderOutput) -> Option<Error> {
        let id = xnode.id;
        log::info!("Undeploying hyperstack device {id} started");
        if let Err(e) = self
            .client
            .delete(format!(
                "https://infrahub-api.nexgencloud.com/v1/core/virtual-machines/{id}"
            ))
            .header("api_key", self.api_key.clone())
            .send()
            .await
            .and_then(|response| response.error_for_status())
        {
            return Some(Error::ReqwestError(e));
        }

        log::info!("Undeploying hyperstack device {id} succeeded");
        None
    }

    async fn ipv4(
        &self,
        xnode: &Self::ProviderOutput,
    ) -> Result<OptionalSupport<Option<Ipv4Addr>>, Error> {
        let id = xnode.id;
        let response = self
            .client
            .get(format!(
                "https://infrahub-api.nexgencloud.com/v1/core/virtual-machines/{id}"
            ))
            .header("api_key", self.api_key.clone())
            .send()
            .await
            .and_then(|response| response.error_for_status())
            .map_err(Error::ReqwestError)?
            .json::<serde_json::Value>()
            .await
            .map_err(Error::ReqwestError)?;

        if let serde_json::Value::Object(map) = &response {
            if let Some(serde_json::Value::Object(instance)) = map.get("instance") {
                if let Some(serde_json::Value::String(floating_ip)) = instance.get("floating_ip") {
                    if let Ok(ip) = Ipv4Addr::from_str(floating_ip) {
                        return Ok(Supported(Some(ip)));
                    }
                }
            }
        };

        Ok(Supported(None))
    }
}

#[derive(Debug, Clone)]
pub struct HyperstackOutput {
    pub id: u64,
}

#[derive(Debug)]
pub enum HyperstackHardware {
    // https://docs.hyperstack.cloud/docs/api-reference/core-resources/virtual-machines/vm-core/create-vms
    VirtualMachine {
        name: String,
        environment_name: String,
        flavor_name: String,
        key_name: String,
    },
}

#[derive(Debug)]
pub enum HyperstackUndeployInput {
    VirtualMachine { id: u64 },
}
