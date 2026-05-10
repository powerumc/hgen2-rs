use cidr::Ipv4Cidr;
use rand::RngExt;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ConfigValue {
    String(String),
    Array(Vec<ConfigValuePair>),
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ConfigValuePair(pub String, pub f64);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub test: AppConfigTest,
    pub http: AppConfigHttp,
    pub params: AppConfigParams,
    pub src: AppConfigEndpoint,
    pub dst: AppConfigEndpoint,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfigTest {
    pub vu: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfigHttp {
    pub req: AppConfigHttpReq,
    pub res: AppConfigHttpRes,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfigHttpReq {
    pub headers: AppConfigHttpReqHeaders,
    pub body: ConfigValue,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfigHttpReqHeaders {
    #[serde(rename = ":host")]
    pub host: ConfigValue,
    #[serde(rename = ":path")]
    pub path: ConfigValue,
    #[serde(rename = ":method")]
    pub method: ConfigValue,

    #[serde(flatten, default)]
    pub headers: HashMap<String, ConfigValue>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfigHttpRes {
    pub headers: AppConfigHttpResHeaders,
    pub body: ConfigValue,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfigHttpResHeaders {
    #[serde(rename = ":status")]
    pub status: ConfigValue,

    #[serde(flatten, default)]
    pub headers: HashMap<String, ConfigValue>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfigParams {
    #[serde(flatten, default)]
    pub values: HashMap<String, ConfigValue>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfigEndpoint {
    pub cidr: Ipv4Cidr,
    pub port: String,
}

impl ConfigValue {
    /// 가중치에 따른 값을 반환
    /// 가중치가 0이면 fallback
    pub fn sample(&self) -> &str {
        match self {
            ConfigValue::String(s) => s,
            ConfigValue::Array(arr) => {
                if arr.is_empty() {
                    return "";
                }

                let ticket = rand::rng().random_range(0.0..1.0);
                let mut cursor = 0.0;
                let mut fallback = None;

                for ConfigValuePair(value, weight) in arr {
                    let w = (*weight).max(0.0);
                    if w == 0.0 {
                        fallback.get_or_insert(value.as_str());
                        continue;
                    }

                    cursor += w;
                    if ticket < cursor {
                        return value.as_str();
                    }
                }

                fallback
                    .or_else(|| arr.last().map(|p| p.0.as_str()))
                    .unwrap_or_default()
            }
        }
    }
}
