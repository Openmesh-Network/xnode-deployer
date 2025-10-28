use std::{fmt::Display, net::Ipv4Addr, str::FromStr};

use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::{
    DeployInput, Error,
    OptionalSupport::{self, Supported},
    XnodeDeployer, XnodeDeployerError,
    utils::XnodeDeployerErrorInner,
};

#[derive(Debug)]
pub enum HivelocityError {
    ResponseNotObject {
        response: serde_json::Value,
    },
    ResponseMissingDeviceId {
        map: serde_json::Map<String, serde_json::Value>,
    },
    ResponseInvalidDeviceId {
        device_id: serde_json::Value,
    },
}

impl Display for HivelocityError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(
            match self {
                HivelocityError::ResponseNotObject { response } => {
                    format!("Hivelocity response not object: {response}")
                }
                HivelocityError::ResponseMissingDeviceId { map } => {
                    format!("Hivelocity response missing device id: {map:?}")
                }
                HivelocityError::ResponseInvalidDeviceId { device_id } => {
                    format!("Hivelocity response invalid device id: {device_id}")
                }
            }
            .as_str(),
        )
    }
}

#[derive(Debug, Clone)]
pub struct HivelocityDeployer {
    client: Client,
    api_key: String,
    hardware: HivelocityHardware,
}

impl HivelocityDeployer {
    pub fn new(api_key: String, hardware: HivelocityHardware) -> Self {
        Self {
            client: Client::new(),
            api_key,
            hardware,
        }
    }
}

impl XnodeDeployer for HivelocityDeployer {
    type ProviderOutput = HivelocityOutput;

    async fn deploy(&self, input: DeployInput) -> Result<Self::ProviderOutput, Error> {
        log::info!(
            "Hivelocity deployment of {input:?} on {hardware:?} started",
            hardware = self.hardware
        );
        let response = match &self.hardware {
            HivelocityHardware::BareMetal {
                location_name,
                period,
                tags,
                product_id,
                hostname,
            } => self
                .client
                .post("https://core.hivelocity.net/api/v2/bare-metal-devices/")
                .json(&json!({
                    "locationName": location_name,
                    "period": period,
                    "tags": tags,
                    "script": input.cloud_init(),
                    "productId": product_id,
                    "osName": "Ubuntu 24.04",
                    "hostname": hostname
                })),
            HivelocityHardware::Compute {
                location_name,
                period,
                tags,
                product_id,
                hostname,
            } => self
                .client
                .post("https://core.hivelocity.net/api/v2/compute/")
                .json(&json!({
                    "locationName": location_name,
                    "period": period,
                    "tags": tags,
                    "script": input.cloud_init(),
                    "productId": product_id,
                    "osName": "Ubuntu 24.04 (VPS)",
                    "hostname": hostname
                })),
        }
        .header("X-API-KEY", self.api_key.clone())
        .send()
        .await
        .and_then(|response| response.error_for_status())
        .map_err(Error::ReqwestError)?
        .json::<serde_json::Value>()
        .await
        .map_err(Error::ReqwestError)?;

        let device_id = match &response {
            serde_json::Value::Object(map) => map
                .get("deviceId")
                .ok_or(Error::XnodeDeployerError(XnodeDeployerError::new(
                    XnodeDeployerErrorInner::HivelocityError(
                        HivelocityError::ResponseMissingDeviceId { map: map.clone() },
                    ),
                )))
                .and_then(|device_id| {
                    match device_id {
                        serde_json::Value::Number(number) => number.as_u64(),
                        _ => None,
                    }
                    .ok_or(Error::XnodeDeployerError(XnodeDeployerError::new(
                        XnodeDeployerErrorInner::HivelocityError(
                            HivelocityError::ResponseInvalidDeviceId {
                                device_id: device_id.clone(),
                            },
                        ),
                    )))
                }),
            _ => Err(Error::XnodeDeployerError(XnodeDeployerError::new(
                XnodeDeployerErrorInner::HivelocityError(HivelocityError::ResponseNotObject {
                    response: response.clone(),
                }),
            ))),
        };
        let device_id = match device_id {
            Ok(device_id) => device_id,
            Err(e) => return Err(e),
        };

        let output = Self::ProviderOutput { device_id };
        log::info!("Hivelocity deployment succeeded: {output:?}");
        Ok(output)
    }

    async fn undeploy(&self, xnode: Self::ProviderOutput) -> Result<(), Error> {
        let device_id = xnode.device_id;
        log::info!("Undeploying hivelocity device {device_id} started");
        let scope = match self.hardware {
            HivelocityHardware::BareMetal { .. } => "bare-metal-devices",
            HivelocityHardware::Compute { .. } => "compute",
        };
        self.client
            .delete(format!(
                "https://core.hivelocity.net/api/v2/{scope}/{device_id}"
            ))
            .header("X-API-KEY", self.api_key.clone())
            .send()
            .await
            .and_then(|response| response.error_for_status())
            .map_err(Error::ReqwestError)?;

        log::info!("Undeploying hivelocity device {device_id} succeeded");
        Ok(())
    }

    async fn ipv4(
        &self,
        xnode: &Self::ProviderOutput,
    ) -> Result<OptionalSupport<Option<Ipv4Addr>>, Error> {
        let device_id = xnode.device_id;
        let scope = match self.hardware {
            HivelocityHardware::BareMetal { .. } => "bare-metal-devices",
            HivelocityHardware::Compute { .. } => "compute",
        };
        let response = self
            .client
            .get(format!(
                "https://core.hivelocity.net/api/v2/{scope}/{device_id}"
            ))
            .header("X-API-KEY", self.api_key.clone())
            .send()
            .await
            .and_then(|response| response.error_for_status())
            .map_err(Error::ReqwestError)?
            .json::<serde_json::Value>()
            .await
            .map_err(Error::ReqwestError)?;

        if let serde_json::Value::Object(map) = &response {
            if let Some(serde_json::Value::String(primary_ip)) = map.get("primaryIp") {
                if let Ok(ip) = Ipv4Addr::from_str(primary_ip) {
                    return Ok(Supported(Some(ip)));
                }
            }
        };

        Ok(Supported(None))
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct HivelocityOutput {
    pub device_id: u64,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub enum HivelocityHardware {
    // https://developers.hivelocity.net/reference/post_bare_metal_device_resource
    BareMetal {
        location_name: String,
        period: String,
        tags: Option<Vec<String>>,
        product_id: u64,
        hostname: String,
    },
    // https://developers.hivelocity.net/reference/post_compute_resource
    Compute {
        location_name: String,
        period: String,
        tags: Option<Vec<String>>,
        product_id: u64,
        hostname: String,
    },
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub enum HivelocityUndeployInput {
    BareMetal { device_id: u64 },
    Compute { device_id: u64 },
}
