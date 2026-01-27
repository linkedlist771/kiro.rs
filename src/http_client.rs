//! HTTP Client 构建模块
//!
//! 提供统一的 HTTP Client 构建功能，支持代理配置

use reqwest::{Client, Proxy};
use std::time::Duration;
use url::Url;

use crate::model::config::TlsBackend;

/// 代理配置
#[derive(Debug, Clone, Default)]
pub struct ProxyConfig {
    /// 代理地址，支持 http/https/socks5
    pub url: String,
    /// 代理认证用户名
    pub username: Option<String>,
    /// 代理认证密码
    pub password: Option<String>,
}

impl ProxyConfig {
    /// 从 url 创建代理配置
    pub fn new(url: impl Into<String>) -> Self {
        Self {
            url: url.into(),
            username: None,
            password: None,
        }
    }

    /// 设置认证信息
    pub fn with_auth(mut self, username: impl Into<String>, password: impl Into<String>) -> Self {
        self.username = Some(username.into());
        self.password = Some(password.into());
        self
    }
}

/// 从完整的代理 URL 解析代理配置
///
/// 支持格式:
/// - `socks5://host:port`
/// - `socks5://user:pass@host:port`
/// - `http://user:pass@host:port#comment`
/// - URL 编码的用户名/密码
///
/// # Arguments
/// * `proxy_url` - 完整的代理 URL 字符串
///
/// # Returns
/// 解析后的代理配置
///
/// # Examples
/// ```ignore
/// let config = parse_proxy_url("socks5://user:pass@127.0.0.1:1080")?;
/// assert_eq!(config.url, "socks5://127.0.0.1:1080");
/// assert_eq!(config.username, Some("user".to_string()));
/// assert_eq!(config.password, Some("pass".to_string()));
/// ```
pub fn parse_proxy_url(proxy_url: &str) -> anyhow::Result<ProxyConfig> {
    let parsed = Url::parse(proxy_url)?;

    // 提取协议
    let scheme = parsed.scheme();
    if !matches!(scheme, "http" | "https" | "socks5" | "socks") {
        anyhow::bail!(
            "不支持的代理协议: {}，支持 http/https/socks5/socks",
            scheme
        );
    }

    // 提取主机和端口
    let host = parsed
        .host_str()
        .ok_or_else(|| anyhow::anyhow!("代理 URL 缺少主机地址"))?;
    let port = parsed
        .port()
        .ok_or_else(|| anyhow::anyhow!("代理 URL 缺少端口"))?;

    // 构建不含认证信息的 URL（reqwest 需要单独设置认证）
    let clean_url = format!("{}://{}:{}", scheme, host, port);

    // 提取并解码用户名和密码
    let username = if !parsed.username().is_empty() {
        Some(
            urlencoding::decode(parsed.username())
                .map(|s| s.into_owned())
                .unwrap_or_else(|_| parsed.username().to_string()),
        )
    } else {
        None
    };

    let password = parsed.password().map(|p| {
        urlencoding::decode(p)
            .map(|s| s.into_owned())
            .unwrap_or_else(|_| p.to_string())
    });

    Ok(ProxyConfig {
        url: clean_url,
        username,
        password,
    })
}

/// 构建 HTTP Client
///
/// # Arguments
/// * `proxy` - 可选的代理配置
/// * `timeout_secs` - 超时时间（秒）
///
/// # Returns
/// 配置好的 reqwest::Client
pub fn build_client(
    proxy: Option<&ProxyConfig>,
    timeout_secs: u64,
    tls_backend: TlsBackend,
) -> anyhow::Result<Client> {
    let mut builder = Client::builder().timeout(Duration::from_secs(timeout_secs));

    if tls_backend == TlsBackend::Rustls {
        builder = builder.use_rustls_tls();
    }

    if let Some(proxy_config) = proxy {
        let mut proxy = Proxy::all(&proxy_config.url)?;

        // 设置代理认证
        if let (Some(username), Some(password)) = (&proxy_config.username, &proxy_config.password) {
            proxy = proxy.basic_auth(username, password);
        }

        builder = builder.proxy(proxy);
        tracing::debug!("HTTP Client 使用代理: {}", proxy_config.url);
    }

    Ok(builder.build()?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_proxy_config_new() {
        let config = ProxyConfig::new("http://127.0.0.1:7890");
        assert_eq!(config.url, "http://127.0.0.1:7890");
        assert!(config.username.is_none());
        assert!(config.password.is_none());
    }

    #[test]
    fn test_proxy_config_with_auth() {
        let config = ProxyConfig::new("socks5://127.0.0.1:1080").with_auth("user", "pass");
        assert_eq!(config.url, "socks5://127.0.0.1:1080");
        assert_eq!(config.username, Some("user".to_string()));
        assert_eq!(config.password, Some("pass".to_string()));
    }

    #[test]
    fn test_build_client_without_proxy() {
        let client = build_client(None, 30, TlsBackend::Rustls);
        assert!(client.is_ok());
    }

    #[test]
    fn test_build_client_with_proxy() {
        let config = ProxyConfig::new("http://127.0.0.1:7890");
        let client = build_client(Some(&config), 30, TlsBackend::Rustls);
        assert!(client.is_ok());
    }

    // ============ parse_proxy_url 测试 ============

    #[test]
    fn test_parse_proxy_url_simple() {
        let config = parse_proxy_url("socks5://127.0.0.1:1080").unwrap();
        assert_eq!(config.url, "socks5://127.0.0.1:1080");
        assert!(config.username.is_none());
        assert!(config.password.is_none());
    }

    #[test]
    fn test_parse_proxy_url_with_auth() {
        let config = parse_proxy_url("socks5://user:pass@127.0.0.1:1080").unwrap();
        assert_eq!(config.url, "socks5://127.0.0.1:1080");
        assert_eq!(config.username, Some("user".to_string()));
        assert_eq!(config.password, Some("pass".to_string()));
    }

    #[test]
    fn test_parse_proxy_url_with_encoded_auth() {
        // URL 编码的密码：p@ss -> p%40ss
        let config = parse_proxy_url("http://user:p%40ss@proxy.example.com:8080").unwrap();
        assert_eq!(config.url, "http://proxy.example.com:8080");
        assert_eq!(config.username, Some("user".to_string()));
        assert_eq!(config.password, Some("p@ss".to_string()));
    }

    #[test]
    fn test_parse_proxy_url_with_fragment() {
        // 带备注的 URL（# 后面的内容应被忽略）
        let config =
            parse_proxy_url("socks5://user:pass@127.0.0.1:1080#webshare-us").unwrap();
        assert_eq!(config.url, "socks5://127.0.0.1:1080");
        assert_eq!(config.username, Some("user".to_string()));
        assert_eq!(config.password, Some("pass".to_string()));
    }

    #[test]
    fn test_parse_proxy_url_http() {
        let config = parse_proxy_url("http://proxy.example.com:3128").unwrap();
        assert_eq!(config.url, "http://proxy.example.com:3128");
    }

    #[test]
    fn test_parse_proxy_url_https() {
        let config = parse_proxy_url("https://secure-proxy.example.com:443").unwrap();
        assert_eq!(config.url, "https://secure-proxy.example.com:443");
    }

    #[test]
    fn test_parse_proxy_url_socks() {
        let config = parse_proxy_url("socks://127.0.0.1:1080").unwrap();
        assert_eq!(config.url, "socks://127.0.0.1:1080");
    }

    #[test]
    fn test_parse_proxy_url_invalid_scheme() {
        let result = parse_proxy_url("ftp://127.0.0.1:21");
        assert!(result.is_err());
        let err = result.err().unwrap().to_string();
        assert!(err.contains("不支持的代理协议"));
    }

    #[test]
    fn test_parse_proxy_url_missing_port() {
        let result = parse_proxy_url("socks5://127.0.0.1");
        assert!(result.is_err());
        let err = result.err().unwrap().to_string();
        assert!(err.contains("缺少端口"));
    }

    #[test]
    fn test_parse_proxy_url_invalid_url() {
        let result = parse_proxy_url("not-a-valid-url");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_proxy_url_only_username() {
        // 只有用户名没有密码的情况
        let config = parse_proxy_url("socks5://onlyuser@127.0.0.1:1080").unwrap();
        assert_eq!(config.url, "socks5://127.0.0.1:1080");
        assert_eq!(config.username, Some("onlyuser".to_string()));
        assert!(config.password.is_none());
    }
}
