//! Kiro OAuth 凭证数据模型
//!
//! 支持从 Kiro IDE 的凭证文件加载，使用 Social 认证方式
//! 支持单凭据和多凭据配置格式

use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
use std::path::PathBuf;

/// Kiro OAuth 凭证
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct KiroCredentials {
    /// 凭据唯一标识符（自增 ID）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<u64>,

    /// 访问令牌
    #[serde(skip_serializing_if = "Option::is_none")]
    pub access_token: Option<String>,

    /// 刷新令牌
    #[serde(skip_serializing_if = "Option::is_none")]
    pub refresh_token: Option<String>,

    /// Profile ARN
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profile_arn: Option<String>,

    /// 过期时间 (RFC3339 格式)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<String>,

    /// 认证方式 (social / idc)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auth_method: Option<String>,

    /// OIDC Client ID (IdC 认证需要)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_id: Option<String>,

    /// OIDC Client Secret (IdC 认证需要)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_secret: Option<String>,

    /// 凭据优先级（数字越小优先级越高，默认为 0）
    #[serde(default)]
    #[serde(skip_serializing_if = "is_zero")]
    pub priority: u32,

    /// 凭据级 Region 配置（用于 OIDC token 刷新）
    /// 未配置时回退到 config.json 的全局 region
    #[serde(skip_serializing_if = "Option::is_none")]
    pub region: Option<String>,

    /// 凭据级 Machine ID 配置（可选）
    /// 未配置时回退到 config.json 的 machineId；都未配置时由 refreshToken 派生
    #[serde(skip_serializing_if = "Option::is_none")]
    pub machine_id: Option<String>,

    /// 凭据级代理 URL（可选）
    /// 支持格式: socks5://user:pass@host:port, http://host:port 等
    /// 未配置时回退到 config.json 的全局代理
    #[serde(skip_serializing_if = "Option::is_none")]
    pub proxy_url: Option<String>,

    /// 邮箱（可选，仅用于标识/显示）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
}

/// 判断是否为零（用于跳过序列化）
fn is_zero(value: &u32) -> bool {
    *value == 0
}

fn canonicalize_auth_method_value(value: &str) -> &str {
    if value.eq_ignore_ascii_case("builder-id") || value.eq_ignore_ascii_case("iam") {
        "idc"
    } else {
        value
    }
}

/// 凭据配置（支持单对象或数组格式）
///
/// 自动识别配置文件格式：
/// - 单对象格式（旧格式，向后兼容）
/// - 数组格式（新格式，支持多凭据）
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum CredentialsConfig {
    /// 单个凭据（旧格式）
    Single(KiroCredentials),
    /// 多凭据数组（新格式）
    Multiple(Vec<KiroCredentials>),
}

impl CredentialsConfig {
    /// 从文件加载凭据配置
    ///
    /// - 如果文件不存在，返回空数组
    /// - 如果文件内容为空，返回空数组
    /// - 支持单对象或数组格式
    pub fn load<P: AsRef<Path>>(path: P) -> anyhow::Result<Self> {
        let path = path.as_ref();

        // 文件不存在时返回空数组
        if !path.exists() {
            if is_sqlite_path(path) {
                if let Some(creds) = try_migrate_legacy_json(path)? {
                    return Ok(CredentialsConfig::Multiple(creds));
                }
            }
            return Ok(CredentialsConfig::Multiple(vec![]));
        }

        // SQLite 文件优先处理
        if is_sqlite_path(path) || is_sqlite_file(path)? {
            let credentials = load_credentials_from_sqlite(path)?;
            return Ok(CredentialsConfig::Multiple(credentials));
        }

        let content = fs::read_to_string(path)?;

        // 文件为空时返回空数组
        if content.trim().is_empty() {
            return Ok(CredentialsConfig::Multiple(vec![]));
        }

        let config = serde_json::from_str(&content)?;
        Ok(config)
    }

    /// 转换为按优先级排序的凭据列表
    pub fn into_sorted_credentials(self) -> Vec<KiroCredentials> {
        match self {
            CredentialsConfig::Single(mut cred) => {
                cred.canonicalize_auth_method();
                vec![cred]
            }
            CredentialsConfig::Multiple(mut creds) => {
                // 按优先级排序（数字越小优先级越高）
                creds.sort_by_key(|c| c.priority);
                for cred in &mut creds {
                    cred.canonicalize_auth_method();
                }
                creds
            }
        }
    }

    /// 获取凭据数量
    pub fn len(&self) -> usize {
        match self {
            CredentialsConfig::Single(_) => 1,
            CredentialsConfig::Multiple(creds) => creds.len(),
        }
    }

    /// 判断是否为空
    pub fn is_empty(&self) -> bool {
        match self {
            CredentialsConfig::Single(_) => false,
            CredentialsConfig::Multiple(creds) => creds.is_empty(),
        }
    }

    /// 判断是否为多凭据格式（数组格式）
    pub fn is_multiple(&self) -> bool {
        matches!(self, CredentialsConfig::Multiple(_))
    }
}

impl KiroCredentials {
    /// 获取默认凭证文件路径
    pub fn default_credentials_path() -> &'static str {
        "data/credentials.db"
    }

    /// 从 JSON 字符串解析凭证
    pub fn from_json(json_string: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json_string)
    }

    /// 从文件加载凭证
    pub fn load<P: AsRef<Path>>(path: P) -> anyhow::Result<Self> {
        let content = fs::read_to_string(path.as_ref())?;
        if content.is_empty() {
            anyhow::bail!("凭证文件为空: {:?}", path.as_ref());
        }
        let credentials = Self::from_json(&content)?;
        Ok(credentials)
    }

    /// 序列化为格式化的 JSON 字符串
    pub fn to_pretty_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }

    pub fn canonicalize_auth_method(&mut self) {
        let auth_method = match &self.auth_method {
            Some(m) => m,
            None => return,
        };

        let canonical = canonicalize_auth_method_value(auth_method);
        if canonical != auth_method {
            self.auth_method = Some(canonical.to_string());
        }
    }
}

pub(crate) fn is_sqlite_path(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|e| e.to_str()).map(|s| s.to_ascii_lowercase()).as_deref(),
        Some("db") | Some("sqlite") | Some("sqlite3")
    )
}

pub(crate) fn is_sqlite_file(path: &Path) -> anyhow::Result<bool> {
    if !path.exists() {
        return Ok(false);
    }

    let mut file = fs::File::open(path)?;
    let mut header = [0u8; 16];
    let n = std::io::Read::read(&mut file, &mut header)?;
    if n < header.len() {
        return Ok(false);
    }
    Ok(&header == b"SQLite format 3\0")
}

fn ensure_parent_dir(path: &Path) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)?;
        }
    }
    Ok(())
}

fn init_schema(conn: &Connection) -> anyhow::Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS credentials (
            id INTEGER PRIMARY KEY,
            access_token TEXT,
            refresh_token TEXT,
            profile_arn TEXT,
            expires_at TEXT,
            auth_method TEXT,
            client_id TEXT,
            client_secret TEXT,
            priority INTEGER NOT NULL DEFAULT 0,
            region TEXT,
            machine_id TEXT,
            proxy_url TEXT,
            email TEXT
        );
        CREATE INDEX IF NOT EXISTS idx_credentials_priority ON credentials(priority);",
    )?;
    // 迁移：为旧表添加 email 列（如果不存在）
    let _ = conn.execute("ALTER TABLE credentials ADD COLUMN email TEXT", []);
    Ok(())
}

pub(crate) fn load_credentials_from_sqlite(path: &Path) -> anyhow::Result<Vec<KiroCredentials>> {
    ensure_parent_dir(path)?;
    let conn = Connection::open(path)?;
    init_schema(&conn)?;

    let mut stmt = conn.prepare(
        "SELECT id, access_token, refresh_token, profile_arn, expires_at, auth_method, \
         client_id, client_secret, priority, region, machine_id, proxy_url, email \
         FROM credentials ORDER BY priority ASC, id ASC",
    )?;

    let rows = stmt.query_map([], |row| {
        let id: Option<i64> = row.get(0)?;
        let priority: Option<i64> = row.get(8)?;
        Ok(KiroCredentials {
            id: id.map(|v| v as u64),
            access_token: row.get(1)?,
            refresh_token: row.get(2)?,
            profile_arn: row.get(3)?,
            expires_at: row.get(4)?,
            auth_method: row.get(5)?,
            client_id: row.get(6)?,
            client_secret: row.get(7)?,
            priority: priority.unwrap_or(0).max(0) as u32,
            region: row.get(9)?,
            machine_id: row.get(10)?,
            proxy_url: row.get(11)?,
            email: row.get(12)?,
        })
    })?;

    let mut credentials = Vec::new();
    for row in rows {
        credentials.push(row?);
    }
    Ok(credentials)
}

pub(crate) fn persist_credentials_to_sqlite(
    path: &Path,
    credentials: &[KiroCredentials],
) -> anyhow::Result<()> {
    ensure_parent_dir(path)?;
    let mut conn = Connection::open(path)?;
    init_schema(&conn)?;

    let tx = conn.transaction()?;
    tx.execute("DELETE FROM credentials", [])?;

    {
        let mut stmt = tx.prepare(
            "INSERT INTO credentials (
                id, access_token, refresh_token, profile_arn, expires_at, auth_method,
                client_id, client_secret, priority, region, machine_id, proxy_url, email
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
        )?;

        for cred in credentials {
            let id = cred.id.map(|v| v as i64);
            stmt.execute(params![
                id,
                cred.access_token.as_deref(),
                cred.refresh_token.as_deref(),
                cred.profile_arn.as_deref(),
                cred.expires_at.as_deref(),
                cred.auth_method.as_deref(),
                cred.client_id.as_deref(),
                cred.client_secret.as_deref(),
                i64::from(cred.priority),
                cred.region.as_deref(),
                cred.machine_id.as_deref(),
                cred.proxy_url.as_deref(),
                cred.email.as_deref(),
            ])?;
        }
    }

    tx.commit()?;
    Ok(())
}

fn try_migrate_legacy_json(path: &Path) -> anyhow::Result<Option<Vec<KiroCredentials>>> {
    let legacy_path = PathBuf::from("credentials.json");
    if !legacy_path.exists() {
        return Ok(None);
    }

    let content = fs::read_to_string(&legacy_path)?;
    if content.trim().is_empty() {
        return Ok(None);
    }

    let config: CredentialsConfig = serde_json::from_str(&content)?;
    let mut credentials = config.into_sorted_credentials();
    for cred in &mut credentials {
        cred.canonicalize_auth_method();
    }

    if let Err(e) = persist_credentials_to_sqlite(path, &credentials) {
        tracing::warn!("迁移 credentials.json 到 SQLite 失败: {}", e);
    }

    Ok(Some(credentials))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_from_json() {
        let json = r#"{
            "accessToken": "test_token",
            "refreshToken": "test_refresh",
            "profileArn": "arn:aws:test",
            "expiresAt": "2024-01-01T00:00:00Z",
            "authMethod": "social"
        }"#;

        let creds = KiroCredentials::from_json(json).unwrap();
        assert_eq!(creds.access_token, Some("test_token".to_string()));
        assert_eq!(creds.refresh_token, Some("test_refresh".to_string()));
        assert_eq!(creds.profile_arn, Some("arn:aws:test".to_string()));
        assert_eq!(creds.expires_at, Some("2024-01-01T00:00:00Z".to_string()));
        assert_eq!(creds.auth_method, Some("social".to_string()));
    }

    #[test]
    fn test_from_json_with_unknown_keys() {
        let json = r#"{
            "accessToken": "test_token",
            "unknownField": "should be ignored"
        }"#;

        let creds = KiroCredentials::from_json(json).unwrap();
        assert_eq!(creds.access_token, Some("test_token".to_string()));
    }

    #[test]
    fn test_to_json() {
        let creds = KiroCredentials {
            id: None,
            access_token: Some("token".to_string()),
            refresh_token: None,
            profile_arn: None,
            expires_at: None,
            auth_method: Some("social".to_string()),
            client_id: None,
            client_secret: None,
            priority: 0,
            region: None,
            machine_id: None,
            proxy_url: None,
        };

        let json = creds.to_pretty_json().unwrap();
        assert!(json.contains("accessToken"));
        assert!(json.contains("authMethod"));
        assert!(!json.contains("refreshToken"));
        // priority 为 0 时不序列化
        assert!(!json.contains("priority"));
    }

    #[test]
    fn test_default_credentials_path() {
        assert_eq!(
            KiroCredentials::default_credentials_path(),
            "data/credentials.db"
        );
    }

    #[test]
    fn test_priority_default() {
        let json = r#"{"refreshToken": "test"}"#;
        let creds = KiroCredentials::from_json(json).unwrap();
        assert_eq!(creds.priority, 0);
    }

    #[test]
    fn test_priority_explicit() {
        let json = r#"{"refreshToken": "test", "priority": 5}"#;
        let creds = KiroCredentials::from_json(json).unwrap();
        assert_eq!(creds.priority, 5);
    }

    #[test]
    fn test_credentials_config_single() {
        let json = r#"{"refreshToken": "test", "expiresAt": "2025-12-31T00:00:00Z"}"#;
        let config: CredentialsConfig = serde_json::from_str(json).unwrap();
        assert!(matches!(config, CredentialsConfig::Single(_)));
        assert_eq!(config.len(), 1);
    }

    #[test]
    fn test_credentials_config_multiple() {
        let json = r#"[
            {"refreshToken": "test1", "priority": 1},
            {"refreshToken": "test2", "priority": 0}
        ]"#;
        let config: CredentialsConfig = serde_json::from_str(json).unwrap();
        assert!(matches!(config, CredentialsConfig::Multiple(_)));
        assert_eq!(config.len(), 2);
    }

    #[test]
    fn test_credentials_config_priority_sorting() {
        let json = r#"[
            {"refreshToken": "t1", "priority": 2},
            {"refreshToken": "t2", "priority": 0},
            {"refreshToken": "t3", "priority": 1}
        ]"#;
        let config: CredentialsConfig = serde_json::from_str(json).unwrap();
        let list = config.into_sorted_credentials();

        // 验证按优先级排序
        assert_eq!(list[0].refresh_token, Some("t2".to_string())); // priority 0
        assert_eq!(list[1].refresh_token, Some("t3".to_string())); // priority 1
        assert_eq!(list[2].refresh_token, Some("t1".to_string())); // priority 2
    }

    // ============ Region 字段测试 ============

    #[test]
    fn test_region_field_parsing() {
        // 测试解析包含 region 字段的 JSON
        let json = r#"{
            "refreshToken": "test_refresh",
            "region": "us-east-1"
        }"#;

        let creds = KiroCredentials::from_json(json).unwrap();
        assert_eq!(creds.refresh_token, Some("test_refresh".to_string()));
        assert_eq!(creds.region, Some("us-east-1".to_string()));
    }

    #[test]
    fn test_region_field_missing_backward_compat() {
        // 测试向后兼容：不包含 region 字段的旧格式 JSON
        let json = r#"{
            "refreshToken": "test_refresh",
            "authMethod": "social"
        }"#;

        let creds = KiroCredentials::from_json(json).unwrap();
        assert_eq!(creds.refresh_token, Some("test_refresh".to_string()));
        assert_eq!(creds.region, None);
    }

    #[test]
    fn test_region_field_serialization() {
        // 测试序列化时正确输出 region 字段
        let creds = KiroCredentials {
            id: None,
            access_token: None,
            refresh_token: Some("test".to_string()),
            profile_arn: None,
            expires_at: None,
            auth_method: None,
            client_id: None,
            client_secret: None,
            priority: 0,
            region: Some("eu-west-1".to_string()),
            machine_id: None,
            proxy_url: None,
        };

        let json = creds.to_pretty_json().unwrap();
        assert!(json.contains("region"));
        assert!(json.contains("eu-west-1"));
    }

    #[test]
    fn test_region_field_none_not_serialized() {
        // 测试 region 为 None 时不序列化
        let creds = KiroCredentials {
            id: None,
            access_token: None,
            refresh_token: Some("test".to_string()),
            profile_arn: None,
            expires_at: None,
            auth_method: None,
            client_id: None,
            client_secret: None,
            priority: 0,
            region: None,
            machine_id: None,
            proxy_url: None,
        };

        let json = creds.to_pretty_json().unwrap();
        assert!(!json.contains("region"));
    }

    // ============ MachineId 字段测试 ============

    #[test]
    fn test_machine_id_field_parsing() {
        let machine_id = "a".repeat(64);
        let json = format!(
            r#"{{
                "refreshToken": "test_refresh",
                "machineId": "{machine_id}"
            }}"#
        );

        let creds = KiroCredentials::from_json(&json).unwrap();
        assert_eq!(creds.refresh_token, Some("test_refresh".to_string()));
        assert_eq!(creds.machine_id, Some(machine_id));
    }

    #[test]
    fn test_machine_id_field_serialization() {
        let mut creds = KiroCredentials::default();
        creds.refresh_token = Some("test".to_string());
        creds.machine_id = Some("b".repeat(64));

        let json = creds.to_pretty_json().unwrap();
        assert!(json.contains("machineId"));
    }

    #[test]
    fn test_machine_id_field_none_not_serialized() {
        let mut creds = KiroCredentials::default();
        creds.refresh_token = Some("test".to_string());
        creds.machine_id = None;

        let json = creds.to_pretty_json().unwrap();
        assert!(!json.contains("machineId"));
    }

    #[test]
    fn test_multiple_credentials_with_different_regions() {
        // 测试多凭据场景下不同凭据使用各自的 region
        let json = r#"[
            {"refreshToken": "t1", "region": "us-east-1"},
            {"refreshToken": "t2", "region": "eu-west-1"},
            {"refreshToken": "t3"}
        ]"#;

        let config: CredentialsConfig = serde_json::from_str(json).unwrap();
        let list = config.into_sorted_credentials();

        assert_eq!(list[0].region, Some("us-east-1".to_string()));
        assert_eq!(list[1].region, Some("eu-west-1".to_string()));
        assert_eq!(list[2].region, None);
    }

    #[test]
    fn test_region_field_with_all_fields() {
        // 测试包含所有字段的完整 JSON
        let json = r#"{
            "id": 1,
            "accessToken": "access",
            "refreshToken": "refresh",
            "profileArn": "arn:aws:test",
            "expiresAt": "2025-12-31T00:00:00Z",
            "authMethod": "idc",
            "clientId": "client123",
            "clientSecret": "secret456",
            "priority": 5,
            "region": "ap-northeast-1"
        }"#;

        let creds = KiroCredentials::from_json(json).unwrap();
        assert_eq!(creds.id, Some(1));
        assert_eq!(creds.access_token, Some("access".to_string()));
        assert_eq!(creds.refresh_token, Some("refresh".to_string()));
        assert_eq!(creds.profile_arn, Some("arn:aws:test".to_string()));
        assert_eq!(creds.expires_at, Some("2025-12-31T00:00:00Z".to_string()));
        assert_eq!(creds.auth_method, Some("idc".to_string()));
        assert_eq!(creds.client_id, Some("client123".to_string()));
        assert_eq!(creds.client_secret, Some("secret456".to_string()));
        assert_eq!(creds.priority, 5);
        assert_eq!(creds.region, Some("ap-northeast-1".to_string()));
    }

    #[test]
    fn test_region_roundtrip() {
        // 测试序列化和反序列化的往返一致性
        let original = KiroCredentials {
            id: Some(42),
            access_token: Some("token".to_string()),
            refresh_token: Some("refresh".to_string()),
            profile_arn: None,
            expires_at: None,
            auth_method: Some("social".to_string()),
            client_id: None,
            client_secret: None,
            priority: 3,
            region: Some("us-west-2".to_string()),
            machine_id: Some("c".repeat(64)),
            proxy_url: None,
        };

        let json = original.to_pretty_json().unwrap();
        let parsed = KiroCredentials::from_json(&json).unwrap();

        assert_eq!(parsed.id, original.id);
        assert_eq!(parsed.access_token, original.access_token);
        assert_eq!(parsed.refresh_token, original.refresh_token);
        assert_eq!(parsed.priority, original.priority);
        assert_eq!(parsed.region, original.region);
        assert_eq!(parsed.machine_id, original.machine_id);
    }

    // ============ ProxyUrl 字段测试 ============

    #[test]
    fn test_proxy_url_field_parsing() {
        let json = r#"{
            "refreshToken": "test_refresh",
            "proxyUrl": "socks5://user:pass@127.0.0.1:1080"
        }"#;

        let creds = KiroCredentials::from_json(json).unwrap();
        assert_eq!(creds.refresh_token, Some("test_refresh".to_string()));
        assert_eq!(
            creds.proxy_url,
            Some("socks5://user:pass@127.0.0.1:1080".to_string())
        );
    }

    #[test]
    fn test_proxy_url_field_missing_backward_compat() {
        // 测试向后兼容：不包含 proxyUrl 字段的旧格式 JSON
        let json = r#"{
            "refreshToken": "test_refresh",
            "authMethod": "social"
        }"#;

        let creds = KiroCredentials::from_json(json).unwrap();
        assert_eq!(creds.refresh_token, Some("test_refresh".to_string()));
        assert_eq!(creds.proxy_url, None);
    }

    #[test]
    fn test_proxy_url_field_serialization() {
        let creds = KiroCredentials {
            id: None,
            access_token: None,
            refresh_token: Some("test".to_string()),
            profile_arn: None,
            expires_at: None,
            auth_method: None,
            client_id: None,
            client_secret: None,
            priority: 0,
            region: None,
            machine_id: None,
            proxy_url: Some("http://proxy.example.com:8080".to_string()),
        };

        let json = creds.to_pretty_json().unwrap();
        assert!(json.contains("proxyUrl"));
        assert!(json.contains("http://proxy.example.com:8080"));
    }

    #[test]
    fn test_proxy_url_field_none_not_serialized() {
        let creds = KiroCredentials {
            id: None,
            access_token: None,
            refresh_token: Some("test".to_string()),
            profile_arn: None,
            expires_at: None,
            auth_method: None,
            client_id: None,
            client_secret: None,
            priority: 0,
            region: None,
            machine_id: None,
            proxy_url: None,
        };

        let json = creds.to_pretty_json().unwrap();
        assert!(!json.contains("proxyUrl"));
    }

    #[test]
    fn test_multiple_credentials_with_different_proxies() {
        let json = r#"[
            {"refreshToken": "t1", "proxyUrl": "socks5://127.0.0.1:1080"},
            {"refreshToken": "t2", "proxyUrl": "http://proxy.example.com:8080"},
            {"refreshToken": "t3"}
        ]"#;

        let config: CredentialsConfig = serde_json::from_str(json).unwrap();
        let list = config.into_sorted_credentials();

        assert_eq!(
            list[0].proxy_url,
            Some("socks5://127.0.0.1:1080".to_string())
        );
        assert_eq!(
            list[1].proxy_url,
            Some("http://proxy.example.com:8080".to_string())
        );
        assert_eq!(list[2].proxy_url, None);
    }
}
