use std::fmt::Display;

#[cfg(feature = "hivelocity")]
use crate::hivelocity::HivelocityError;
#[cfg(feature = "hyperstack")]
use crate::hyperstack::HyperstackError;

#[derive(Debug)]
pub enum Error {
    XnodeDeployerError(XnodeDeployerError),
    ReqwestError(reqwest::Error),
}

#[derive(Debug)]
pub struct XnodeDeployerError {
    error: Box<XnodeDeployerErrorInner>,
}

impl Display for XnodeDeployerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.error.fmt(f)
    }
}

#[derive(Debug)]
pub enum XnodeDeployerErrorInner {
    Default,
    #[cfg(feature = "hivelocity")]
    HivelocityError(HivelocityError),
    #[cfg(feature = "hyperstack")]
    HyperstackError(HyperstackError),
}

impl Display for XnodeDeployerErrorInner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(
            match self {
                XnodeDeployerErrorInner::Default => "".to_string(),
                #[cfg(feature = "hivelocity")]
                XnodeDeployerErrorInner::HivelocityError(e) => e.to_string(),
                #[cfg(feature = "hyperstack")]
                XnodeDeployerErrorInner::HyperstackError(e) => e.to_string(),
            }
            .as_str(),
        )
    }
}

impl XnodeDeployerError {
    pub fn new(error: XnodeDeployerErrorInner) -> Self {
        Self {
            error: Box::new(error),
        }
    }
}
