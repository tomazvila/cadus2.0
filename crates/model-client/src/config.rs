//! The endpoint, the credentials and the T5 defaults of one deployment, read
//! from the environment.

use std::time::Duration;

use url::Url;

use crate::{
    API_KEY_VAR, AUTHORING_OUTPUT_TOKENS_VAR, AUTHORING_REASONING_MAX_TOKENS_VAR, BASE_URL_VAR,
    DEFAULT_AUTHORING_OUTPUT_TOKENS, DEFAULT_AUTHORING_REASONING_MAX_TOKENS, DEFAULT_BASE_URL,
    DEFAULT_MODEL, DEFAULT_OUTPUT_TOKENS, DEFAULT_REASONING_MAX_TOKENS, MODEL_VAR, ModelError,
    OUTPUT_TOKENS_VAR, PROVIDER_ORDER_VAR, REASONING_MAX_TOKENS_VAR, ROUTING_HOST, TIMEOUT_SECS,
};

/// One read of the environment: the value of a variable, or `None`.
type Read<'a> = &'a dyn Fn(&str) -> Option<String>;

/// The value of one process environment variable.
fn env_read(name: &str) -> Option<String> {
    std::env::var(name).ok()
}

/// The endpoint, the credentials and the T5 defaults of one deployment.
#[derive(Debug, Clone, PartialEq)]
pub struct ModelConfig {
    /// The base URL, without a trailing `/chat/completions`.
    pub base_url: String,
    /// The bearer token of the endpoint.
    pub api_key: String,
    /// The model id the request names.
    pub model: String,
    /// The output ceiling of the first attempt (T4, T5).
    pub output_tokens: u32,
    /// The reasoning ceiling of every attempt (T5).
    pub reasoning_max_tokens: u32,
    /// The pinned provider order (T5). It is empty for a non-routing endpoint.
    pub provider_order: Vec<String>,
    /// The client-side bound of one attempt.
    pub timeout: Duration,
}

impl ModelConfig {
    /// Read the endpoint from the environment.
    ///
    /// # Errors
    ///
    /// Returns [`ModelError::Config`] when [`API_KEY_VAR`] is absent or empty,
    /// when the base URL does not parse, when a token bound is not a whole
    /// number, and when the endpoint is the routing host and
    /// [`PROVIDER_ORDER_VAR`] names no provider. T5 makes the order a shipped
    /// default, so an empty one is an operator mistake and not a silent
    /// fallback to 1.0's unset routing.
    pub fn from_env() -> Result<Self, ModelError> {
        Self::from_reads(&env_read)
    }

    /// `from_env` over any reader of variables.
    fn from_reads(read: Read<'_>) -> Result<Self, ModelError> {
        let base_url = var_or(read, BASE_URL_VAR, DEFAULT_BASE_URL);
        let api_key = var_or(read, API_KEY_VAR, "");
        if api_key.is_empty() {
            return Err(ModelError::Config(format!("{API_KEY_VAR} is empty")));
        }
        let provider_order: Vec<String> = var_or(read, PROVIDER_ORDER_VAR, "")
            .split(',')
            .map(|name| name.trim().to_owned())
            .filter(|name| !name.is_empty())
            .collect();
        if is_routing_host(&base_url)? && provider_order.is_empty() {
            return Err(ModelError::Config(format!(
                "{PROVIDER_ORDER_VAR} is empty; T5 pins the provider order of {ROUTING_HOST}"
            )));
        }
        Ok(Self {
            base_url,
            api_key,
            model: var_or(read, MODEL_VAR, DEFAULT_MODEL),
            output_tokens: number(read, OUTPUT_TOKENS_VAR, DEFAULT_OUTPUT_TOKENS)?,
            reasoning_max_tokens: number(
                read,
                REASONING_MAX_TOKENS_VAR,
                DEFAULT_REASONING_MAX_TOKENS,
            )?,
            provider_order,
            timeout: Duration::from_secs(TIMEOUT_SECS),
        })
    }

    /// Read the endpoint from the environment for an AUTHORING pass.
    ///
    /// The endpoint, the key, the model and the provider order are the ones
    /// [`ModelConfig::from_env`] reads. The two token bounds are not: authoring
    /// takes [`AUTHORING_OUTPUT_TOKENS_VAR`] and
    /// [`AUTHORING_REASONING_MAX_TOKENS_VAR`], and it NEVER takes the diagnosis
    /// values. A diagnosis budget of 600 output tokens with a reasoning ceiling
    /// of 600 beside it leaves zero visible tokens for an authored document.
    ///
    /// # Errors
    ///
    /// Returns every [`ModelError::Config`] of [`ModelConfig::from_env`], and
    /// one more when an authoring token bound is not a whole number.
    pub fn authoring_from_env() -> Result<Self, ModelError> {
        Self::authoring_from_reads(&env_read)
    }

    /// `authoring_from_env` over any reader of variables.
    fn authoring_from_reads(read: Read<'_>) -> Result<Self, ModelError> {
        Ok(Self {
            output_tokens: number(
                read,
                AUTHORING_OUTPUT_TOKENS_VAR,
                DEFAULT_AUTHORING_OUTPUT_TOKENS,
            )?,
            reasoning_max_tokens: number(
                read,
                AUTHORING_REASONING_MAX_TOKENS_VAR,
                DEFAULT_AUTHORING_REASONING_MAX_TOKENS,
            )?,
            ..Self::from_reads(read)?
        })
    }
}

/// The environment value, or the default when it is absent or blank.
fn var_or(read: Read<'_>, name: &str, fallback: &str) -> String {
    match read(name) {
        Some(raw) if !raw.trim().is_empty() => raw.trim().to_owned(),
        _ => fallback.to_owned(),
    }
}

/// One whole-number knob. A present value that is not a number is an error, so
/// an operator typo never falls back to the default in silence.
fn number(read: Read<'_>, name: &str, fallback: u32) -> Result<u32, ModelError> {
    let raw = var_or(read, name, "");
    if raw.is_empty() {
        return Ok(fallback);
    }
    raw.parse()
        .map_err(|_| ModelError::Config(format!("{name} must be a whole number, not {raw:?}")))
}

/// Does this endpoint take the OpenRouter `provider` and `reasoning` blocks?
///
/// The check reads the parsed host and compares the WHOLE name, so
/// `https://openrouter.ai.attacker.example/v1` is not the routing host and
/// `https://openrouter.ai/api/v1` is.
///
/// # Errors
///
/// Returns [`ModelError::Config`] when the URL does not parse or names no host.
pub fn is_routing_host(base_url: &str) -> Result<bool, ModelError> {
    let url = Url::parse(base_url)
        .map_err(|err| ModelError::Config(format!("{BASE_URL_VAR} does not parse: {err}")))?;
    let host = url
        .host_str()
        .ok_or_else(|| ModelError::Config(format!("{BASE_URL_VAR} names no host")))?;
    Ok(host.eq_ignore_ascii_case(ROUTING_HOST))
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::time::Duration;

    use super::{ModelConfig, ROUTING_HOST, env_read, is_routing_host};

    /// A reader over a literal list of variables.
    fn reads(vars: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
        let map: BTreeMap<String, String> = vars
            .iter()
            .map(|(name, value)| ((*name).to_owned(), (*value).to_owned()))
            .collect();
        move |name: &str| map.get(name).cloned()
    }

    /// The message of a configuration error.
    fn message(outcome: Result<ModelConfig, super::ModelError>) -> String {
        outcome
            .map(|cfg| cfg.base_url)
            .unwrap_or_else(|err| err.to_string())
    }

    /// The routing check reads the parsed host, not a substring of the URL.
    ///
    /// 1.0's `"openrouter.ai" in url` accepted the attacker host of the third
    /// case, which sends the API key to it (spec section 6.4).
    #[test]
    fn the_routing_host_is_the_parsed_host() {
        assert_eq!(ROUTING_HOST, "openrouter.ai");
        assert_eq!(
            is_routing_host("https://openrouter.ai/api/v1").ok(),
            Some(true)
        );
        assert_eq!(
            is_routing_host("https://OpenRouter.ai/api/v1").ok(),
            Some(true)
        );
        assert_eq!(
            is_routing_host("https://openrouter.ai.attacker.example/v1").ok(),
            Some(false)
        );
        assert_eq!(is_routing_host("http://10.8.0.3:8080/v1").ok(), Some(false));
        assert!(is_routing_host("not a url").is_err());
        assert_eq!(
            is_routing_host("data:text").unwrap_err().to_string(),
            "model configuration error: OPENAI_BASE_URL names no host"
        );
    }

    /// The defaults fill every absent variable of a local endpoint, and blank
    /// values count as absent.
    #[test]
    fn the_defaults_fill_every_absent_variable() {
        let cfg = ModelConfig::from_reads(&reads(&[
            ("OPENAI_API_KEY", " key "),
            ("OPENAI_BASE_URL", "http://10.8.0.3:8080/v1"),
            ("OPENAI_MODEL", "  "),
        ]))
        .unwrap();
        assert_eq!(cfg.api_key, "key");
        assert_eq!(cfg.model, "deepseek/deepseek-v4-pro");
        assert_eq!(cfg.output_tokens, 600);
        assert_eq!(cfg.reasoning_max_tokens, 600);
        assert!(cfg.provider_order.is_empty());
        assert_eq!(cfg.timeout, Duration::from_secs(60));

        let authoring = ModelConfig::authoring_from_reads(&reads(&[
            ("OPENAI_API_KEY", "key"),
            ("OPENAI_BASE_URL", "http://10.8.0.3:8080/v1"),
            ("AUTHORING_OUTPUT_TOKENS", "5000"),
        ]))
        .unwrap();
        assert_eq!(authoring.output_tokens, 5000);
        assert_eq!(authoring.reasoning_max_tokens, 2000);
    }

    /// The routing host takes the provider order, split on commas and
    /// trimmed, and refuses an empty one.
    #[test]
    fn the_routing_host_takes_the_provider_order() {
        let cfg = ModelConfig::from_reads(&reads(&[
            ("OPENAI_API_KEY", "key"),
            ("OPENROUTER_PROVIDER_ORDER", " deepseek , , fireworks"),
        ]))
        .unwrap();
        assert_eq!(cfg.base_url, "https://openrouter.ai/api/v1");
        assert_eq!(cfg.provider_order, vec!["deepseek", "fireworks"]);

        assert_eq!(
            message(ModelConfig::from_reads(&reads(&[(
                "OPENAI_API_KEY",
                "key"
            )]))),
            "model configuration error: OPENROUTER_PROVIDER_ORDER is empty; T5 pins the provider \
             order of openrouter.ai"
        );
    }

    /// An absent key, a base URL that does not parse, and a token bound that
    /// is not a whole number are the three configuration errors.
    #[test]
    fn every_bad_variable_names_itself() {
        assert_eq!(
            message(ModelConfig::from_reads(&reads(&[]))),
            "model configuration error: OPENAI_API_KEY is empty"
        );
        assert_eq!(
            message(ModelConfig::from_reads(&reads(&[
                ("OPENAI_API_KEY", "key"),
                ("OPENAI_BASE_URL", "not a url"),
            ]))),
            "model configuration error: OPENAI_BASE_URL does not parse: relative URL without a base"
        );
        let local = [
            ("OPENAI_API_KEY", "key"),
            ("OPENAI_BASE_URL", "http://10.8.0.3:8080/v1"),
            ("DIAGNOSIS_OUTPUT_TOKENS", "six hundred"),
        ];
        assert_eq!(
            message(ModelConfig::from_reads(&reads(&local))),
            "model configuration error: DIAGNOSIS_OUTPUT_TOKENS must be a whole number, not \
             \"six hundred\""
        );
        for (name, value) in [
            ("DIAGNOSIS_REASONING_MAX_TOKENS", "6e2"),
            ("AUTHORING_OUTPUT_TOKENS", "4k"),
            ("AUTHORING_REASONING_MAX_TOKENS", "-1"),
        ] {
            let vars = [
                ("OPENAI_API_KEY", "key"),
                ("OPENAI_BASE_URL", "http://10.8.0.3:8080/v1"),
                (name, value),
            ];
            assert_eq!(
                message(ModelConfig::authoring_from_reads(&reads(&vars))),
                format!("model configuration error: {name} must be a whole number, not {value:?}")
            );
        }
    }

    /// The two environment constructors are the reader constructors over the
    /// process environment.
    #[test]
    fn from_env_reads_the_process_environment() {
        assert_eq!(
            message(ModelConfig::from_env()),
            message(ModelConfig::from_reads(&env_read))
        );
        assert_eq!(
            message(ModelConfig::authoring_from_env()),
            message(ModelConfig::authoring_from_reads(&env_read))
        );
    }
}
