use std::collections::HashMap;
use cidr::Ipv4Cidr;
use rand::RngExt;
use serde::{Deserialize, Serialize};

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
    pub vu: usize
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfigHttp {
    pub req: AppConfigHttpReq,
    pub res: AppConfigHttpRes,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfigHttpReq {
    pub headers: AppConfigHttpReqHeaders,
    pub body: ConfigValue
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
    pub headers: HashMap<String, ConfigValue>
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfigHttpRes {
    pub headers: AppConfigHttpResHeaders,
    pub body: ConfigValue
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfigHttpResHeaders {
    #[serde(rename = ":status")]
    pub status: ConfigValue,
    
    #[serde(flatten, default)]
    pub headers: HashMap<String, ConfigValue>
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfigParams {
    #[serde(flatten, default)]
    pub values: HashMap<String, ConfigValue>
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfigEndpoint {
    pub cidr: Ipv4Cidr,
    pub port: String
}

impl ConfigValue {
    pub fn sample(&self) -> &str {
        match self {
            ConfigValue::String(s) => s,
            ConfigValue::Array(arr) => {
                if arr.is_empty() {
                    return "";
                }

                let total_weight: f64 = arr
                    .iter()
                    .map(|ConfigValuePair(_, w)| (*w).max(0.0))
                    .sum();

                if total_weight <= 0.0 {
                    return arr[0].0.as_str();
                }

                let mut rng = rand::rng();
                let mut ticket = rng.random_range(0.0..total_weight);

                for ConfigValuePair(value, weight) in arr {
                    let w = (*weight).max(0.0);
                    if w == 0.0 {
                        continue;
                    }

                    if ticket < w {
                        return value.as_str();
                    }
                    ticket -= w;
                }

                arr.last().map(|p| p.0.as_str()).unwrap_or_default()
            }
        }
    }
}

impl AppConfigParams {
    fn sample_raw(&self, key: &str) -> Option<String> {
        self.values.get(key).map(|v| v.sample().to_string())
    }

    fn render_template_with_depth(&self, input: &str, depth: usize) -> String {
        if depth == 0 {
            return input.to_string();
        }

        let mut out = String::with_capacity(input.len());
        let mut i = 0usize;

        while let Some(rel_open) = input[i..].find('{') {
            let open = i + rel_open;
            out.push_str(&input[i..open]);

            if let Some(rel_close) = input[open + 1..].find('}') {
                let close = open + 1 + rel_close;
                let key = &input[open + 1..close];

                if key.is_empty() {
                    out.push_str("{}");
                } else if let Some(sampled) = self.sample_raw(key) {
                    let resolved = self.render_template_with_depth(&sampled, depth - 1);
                    out.push_str(&resolved);
                } else {
                    out.push_str(&input[open..=close]);
                }

                i = close + 1;
            } else {
                out.push_str(&input[open..]);
                i = input.len();
                break;
            }
        }

        if i < input.len() {
            out.push_str(&input[i..]);
        }

        out
    }

    pub fn render_template(&self, input: &str) -> String {
        self.render_template_with_depth(input, 8)
    }

    pub fn sample_and_render(&self, value: &ConfigValue) -> String {
        let sampled = value.sample();
        self.render_template(sampled)
    }
}

impl AppConfigHttpReqHeaders {
    pub fn get_host(&self) -> &str {
        self.host.sample()
    }
}
