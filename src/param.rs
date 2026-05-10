use crate::config::{AppConfigParams, ConfigValue};
use regex::{Captures, Regex};
use std::collections::HashMap;

/// params 값을 보관하고 템플릿 문자열을 치환하는 작업 처리
#[derive(Clone)]
pub struct ParamResolver {
    params: HashMap<String, ConfigValue>,
    template_regex: Regex,
}

impl ParamResolver {
    pub fn new(params: AppConfigParams) -> Self {
        Self {
            params: params.values.clone(),
            template_regex: Regex::new(r"\{([A-Za-z_][A-Za-z0-9_]*)}").unwrap(),
        }
    }

    /// 가중치에 따른 값을 반환 후 템플릿 문자열(`{...}`)을 실제 값으로 치환
    /// (재귀적 처리는 최대 5회)
    pub fn render_sample(&self, value: &ConfigValue) -> String {
        let sampled = value.sample();
        self.render_template_with_depth(sampled, 5)
    }

    fn sample_raw(&self, key: &str) -> Option<String> {
        self.params.get(key).map(|v| v.sample().to_string())
    }

    fn render_template_with_depth(&self, input: &str, depth: usize) -> String {
        if depth == 0 {
            return input.to_string();
        }

        self.template_regex
            .replace_all(input, |caps: &Captures| {
                let key = &caps[1];
                self.sample_raw(key)
                    .map(|sampled| self.render_template_with_depth(&sampled, depth - 1))
                    .unwrap_or_else(|| caps[0].to_string())
            })
            .into_owned()
    }
}
