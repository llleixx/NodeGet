use crate::config::deserialize_uuid_or_auto;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::Path;
use tokio::fs;

pub const DEFAULT_CONNECT_TIMEOUT_MS: u64 = 1000;
pub const DEFAULT_NTP_SERVER: &str = "time.pool.aliyun.com";

// Agent 配置结构体，定义 Agent 的运行参数
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct AgentConfig {
    // 日志级别
    pub log_level: Option<String>,
    // 动态监控数据上报间隔（毫秒），默认 1000（1 秒）
    pub dynamic_report_interval_ms: Option<u64>,
    // 动态监控摘要数据上报间隔（毫秒），默认 1000（1 秒）
    // 必须是 dynamic_report_interval_ms 的因数（即 dynamic_report_interval_ms 是它的整数倍）
    pub dynamic_summary_report_interval_ms: Option<u64>,
    // 静态监控数据上报间隔（毫秒），默认 300000（5 分钟）
    pub static_report_interval_ms: Option<u64>,

    // Agent UUID，默认自动生成
    #[serde(deserialize_with = "deserialize_uuid_or_auto")]
    pub agent_uuid: uuid::Uuid,

    // WebSocket 连接超时时间（毫秒）
    pub connect_timeout_ms: Option<u64>,

    // 执行命令输出的最大字符数限制
    pub exec_max_character: Option<usize>,

    // 终端 Shell
    pub terminal_shell: Option<String>,

    // IP 地址获取服务提供商
    pub ip_provider: Option<IpProvider>,

    // NTP 服务器地址，默认使用 time.pool.aliyun.com
    pub ntp_server: Option<String>,

    // 服务器列表
    pub server: Option<Vec<Server>>,
}

// 服务器配置结构体，定义 Agent 连接的服务器信息
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Server {
    // 服务器名称
    pub name: String, // Only For Agent
    // 服务器 UUID，用于连接时校验服务器身份
    pub server_uuid: String,
    // 认证令牌
    pub token: String,
    // WebSocket 连接地址
    pub ws_url: String,

    // 是否允许执行任务
    pub allow_task: Option<bool>,

    // 是否允许 ICMP Ping
    pub allow_icmp_ping: Option<bool>,
    // 是否允许 TCP Ping
    pub allow_tcp_ping: Option<bool>,
    // 是否允许 HTTP Ping
    pub allow_http_ping: Option<bool>,

    // 是否允许 Web Shell
    pub allow_web_shell: Option<bool>,
    // 是否允许阅读配置
    pub allow_read_config: Option<bool>, // Dangerous
    // 是否允许编辑配置
    pub allow_edit_config: Option<bool>, // Dangerous
    // 是否允许执行命令
    pub allow_execute: Option<bool>, // Dangerous
    // 是否允许 HTTP 请求任务
    pub allow_http_request: Option<bool>, // Dangerous

    // 是否允许获取 IP 地址
    pub allow_ip: Option<bool>,
    // 是否允许获取版本信息
    pub allow_version: Option<bool>,
}

// IP 地址获取服务提供商枚举
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "lowercase")]
pub enum IpProvider {
    IpInfo,
    Cloudflare,
}

impl AgentConfig {
    #[must_use]
    pub fn resolved_connect_timeout_ms(&self) -> u64 {
        self.connect_timeout_ms
            .unwrap_or(DEFAULT_CONNECT_TIMEOUT_MS)
    }

    #[must_use]
    pub fn resolved_ntp_server(&self) -> &str {
        self.ntp_server
            .as_deref()
            .map(str::trim)
            .filter(|server| !server.is_empty())
            .unwrap_or(DEFAULT_NTP_SERVER)
    }

    #[must_use]
    pub fn resolved_terminal_shell(&self) -> Option<&str> {
        self.terminal_shell
            .as_deref()
            .map(str::trim)
            .filter(|shell| !shell.is_empty())
    }

    /// 从指定路径读取并解析代理配置
    ///
    /// 若配置文件中 `agent_uuid` 为 `"auto_gen"`，则会生成随机 `UUIDv4`
    /// 并直接覆盖原配置文件，后续启动不再触发自动生成。
    ///
    /// # Errors
    ///
    /// 当文件读取失败或TOML解析失败时返回错误
    pub async fn get_and_parse_config(
        path: impl AsRef<Path>,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let content = fs::read_to_string(path.as_ref()).await?;

        // 检查并替换 auto_gen
        let value: toml::Value = toml::from_str(&content)?;
        let is_auto_gen = value
            .get("agent_uuid")
            .and_then(|v| v.as_str())
            .is_some_and(|s| s.eq_ignore_ascii_case("auto_gen"));

        let config_content = if is_auto_gen {
            let new_uuid = uuid::Uuid::new_v4().to_string();
            let mut new_content = String::with_capacity(content.len() + 32);
            for line in content.lines() {
                let trimmed = line.trim_start();
                if trimmed.starts_with('#') || trimmed.is_empty() {
                    new_content.push_str(line);
                    new_content.push('\n');
                    continue;
                }
                let key_end = trimmed
                    .find(|c: char| c == '=' || c.is_ascii_whitespace())
                    .unwrap_or(trimmed.len());
                if key_end == 10
                    && trimmed[..key_end].eq_ignore_ascii_case("agent_uuid")
                    && let Some(eq_pos) = line.find('=')
                {
                    let before = &line[..=eq_pos];
                    let after = &line[eq_pos + 1..];
                    let after_trimmed = after.trim_start();
                    if let Some(first_char) = after_trimmed.chars().next()
                        && (first_char == '"' || first_char == '\'')
                    {
                        let rest = &after_trimmed[1..];
                        if rest.len() >= 8 && rest[..8].eq_ignore_ascii_case("auto_gen") {
                            let after_value = &rest[8..];
                            new_content.push_str(before);
                            new_content.push(' ');
                            new_content.push(first_char);
                            new_content.push_str(&new_uuid);
                            new_content.push_str(after_value);
                            new_content.push('\n');
                            continue;
                        }
                    }
                }
                new_content.push_str(line);
                new_content.push('\n');
            }
            fs::write(path.as_ref(), &new_content).await?;
            new_content
        } else {
            content
        };

        let config: Self = toml::from_str(&config_content)?;

        if config.connect_timeout_ms == Some(0) {
            return Err("connect_timeout_ms must be greater than 0".into());
        }

        // 校验 server name 不能重复
        if let Some(servers) = &config.server {
            let mut seen = HashSet::with_capacity(servers.len());
            for server in servers {
                if !seen.insert(&server.name) {
                    return Err(format!("Duplicate server name '{}' in config", server.name).into());
                }
            }
        }

        // 校验 dynamic_report_interval_ms 必须是 dynamic_summary_report_interval_ms 的整数倍
        {
            let dynamic_interval = config.dynamic_report_interval_ms.unwrap_or(1000);
            let summary_interval = config.dynamic_summary_report_interval_ms.unwrap_or(1000);
            if summary_interval == 0 {
                return Err("dynamic_summary_report_interval_ms must be greater than 0".into());
            }
            if !dynamic_interval.is_multiple_of(summary_interval) {
                return Err(format!(
                    "dynamic_report_interval_ms ({dynamic_interval}) must be an integer multiple of dynamic_summary_report_interval_ms ({summary_interval})"
                )
                    .into());
            }
        }

        Ok(config)
    }
}

#[cfg(test)]
mod tests {
    use super::{AgentConfig, DEFAULT_CONNECT_TIMEOUT_MS, DEFAULT_NTP_SERVER};

    fn minimal_config() -> AgentConfig {
        AgentConfig {
            log_level: Some("info".to_owned()),
            dynamic_report_interval_ms: None,
            dynamic_summary_report_interval_ms: None,
            static_report_interval_ms: None,
            agent_uuid: uuid::Uuid::nil(),
            connect_timeout_ms: None,
            exec_max_character: None,
            terminal_shell: None,
            ip_provider: None,
            ntp_server: None,
            server: None,
        }
    }

    #[test]
    fn resolved_defaults_match_documented_values() {
        let config = minimal_config();

        assert_eq!(
            config.resolved_connect_timeout_ms(),
            DEFAULT_CONNECT_TIMEOUT_MS
        );
        assert_eq!(config.resolved_ntp_server(), DEFAULT_NTP_SERVER);
        assert_eq!(config.resolved_terminal_shell(), None);
    }

    #[test]
    fn resolved_values_trim_configured_strings() {
        let mut config = minimal_config();
        config.connect_timeout_ms = Some(2500);
        config.ntp_server = Some(" ntp.example.com ".to_owned());
        config.terminal_shell = Some(" /usr/bin/zsh ".to_owned());

        assert_eq!(config.resolved_connect_timeout_ms(), 2500);
        assert_eq!(config.resolved_ntp_server(), "ntp.example.com");
        assert_eq!(config.resolved_terminal_shell(), Some("/usr/bin/zsh"));
    }
}
