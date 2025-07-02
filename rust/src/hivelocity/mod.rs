use std::{fmt::Display, time::Duration};

use reqwest::Client;
use serde_json::json;
use tokio::time::sleep;

use crate::{
    DeployInput, DeployOutput, Error, XnodeDeployer, XnodeDeployerError,
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

    pub async fn undeploy(&self, input: HivelocityUndeployInput) -> Option<Error> {
        let scope = match input {
            HivelocityUndeployInput::BareMetal { device_id } => {
                format!("bare-metal-devices/{device_id}")
            }
            HivelocityUndeployInput::Compute { device_id } => format!("compute/{device_id}"),
        };
        self.client
            .delete(format!("https://core.hivelocity.net/api/v2/{scope}"))
            .header("X-API-KEY", self.api_key.clone())
            .send()
            .await
            .err()
            .map(Error::ReqwestError)
    }
}

impl XnodeDeployer for HivelocityDeployer {
    type ProviderOutput = HivelocityOutput;

    async fn deploy(
        &self,
        input: DeployInput,
    ) -> Result<DeployOutput<Self::ProviderOutput>, Error> {
        log::info!(
            "Deploying Xnode with configuration {input:?} on {hardware:?}",
            hardware = self.hardware
        );
        let mut response = match &self.hardware {
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

        let mut ip = "0.0.0.0".to_string();
        while ip == "0.0.0.0" {
            log::info!("Getting ip address of hivelocity device {device_id}",);
            if let serde_json::Value::Object(map) = &response {
                if let Some(serde_json::Value::String(primary_ip)) = map.get("primaryIp") {
                    ip = primary_ip.clone();
                }
            };

            sleep(Duration::from_secs(1)).await;
            let scope = match self.hardware {
                HivelocityHardware::BareMetal { .. } => "bare-metal-devices",
                HivelocityHardware::Compute { .. } => "compute",
            };
            response = self
                .client
                .get(format!(
                    "https://core.hivelocity.net/api/v2/{scope}/{device_id}"
                ))
                .header("X-API-KEY", self.api_key.clone())
                .send()
                .await
                .map_err(Error::ReqwestError)?
                .json::<serde_json::Value>()
                .await
                .map_err(Error::ReqwestError)?;
        }

        let output = DeployOutput::<Self::ProviderOutput> {
            ip,
            provider: HivelocityOutput { device_id },
        };
        log::info!("Hivelocity deployment succeeded: {output:?}");
        Ok(output)
    }
}

#[derive(Debug)]
pub struct HivelocityOutput {
    pub device_id: u64,
}

#[derive(Debug)]
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

#[derive(Debug)]
pub enum HivelocityUndeployInput {
    BareMetal { device_id: u64 },
    Compute { device_id: u64 },
}
