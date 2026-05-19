use crate::config::{add_log, DdnsConfig};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use hmac::{Hmac, Mac};
use reqwest::Client;
use serde::Deserialize;
use sha2::Sha256;
use std::collections::BTreeMap;
use std::time::{SystemTime, UNIX_EPOCH};
use url::Url;

type HmacSha256 = Hmac<Sha256>;

const ENDPOINT: &str = "https://alidns.aliyuncs.com/";
const API_VERSION: &str = "2015-01-09";

#[derive(Debug, Deserialize)]
struct AliyunResponse {
    #[serde(rename = "DomainRecords")]
    domain_records: Option<DomainRecords>,
    #[serde(rename = "RecordId")]
    #[allow(dead_code)]
    record_id: Option<String>,
    #[serde(rename = "RequestId")]
    #[allow(dead_code)]
    request_id: Option<String>,
    #[serde(rename = "Code")]
    code: Option<String>,
    #[serde(rename = "Message")]
    message: Option<String>,
}

#[derive(Debug, Deserialize)]
struct DomainRecords {
    #[serde(rename = "Record")]
    record: Vec<DnsRecord>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct DnsRecord {
    #[serde(rename = "RecordId")]
    pub record_id: String,
    #[serde(rename = "RR")]
    pub rr: String,
    #[serde(rename = "Type")]
    pub record_type: String,
    #[serde(rename = "Value")]
    pub value: String,
    #[serde(rename = "TTL")]
    #[allow(dead_code)]
    pub ttl: u32,
    #[serde(rename = "Status")]
    #[allow(dead_code)]
    pub status: Option<String>,
}

pub struct AliyunDdns {
    config: DdnsConfig,
    client: Client,
}

impl AliyunDdns {
    pub fn new(config: DdnsConfig) -> Self {
        Self {
            config,
            client: Client::new(),
        }
    }

    fn record_rr(config: &DdnsConfig) -> String {
        let sub_domain = config.sub_domain.trim();
        if sub_domain.is_empty() {
            "@".to_string()
        } else {
            sub_domain.to_string()
        }
    }

    fn record_fqdn(config: &DdnsConfig) -> String {
        let domain = config.domain.trim();
        let sub_domain = config.sub_domain.trim();
        if sub_domain.is_empty() {
            domain.to_string()
        } else {
            format!("{}.{}", sub_domain, domain)
        }
    }

    pub fn acme_challenge_rr(identifier: &str, dns_domain: &str) -> Result<String, String> {
        let identifier = identifier
            .trim()
            .trim_start_matches("*.")
            .trim_end_matches('.')
            .to_ascii_lowercase();
        let dns_domain = dns_domain.trim().trim_end_matches('.').to_ascii_lowercase();
        if identifier.is_empty() || dns_domain.is_empty() {
            return Err("ACME 域名和 DNS 主域名不能为空".to_string());
        }
        if identifier == dns_domain {
            return Ok("_acme-challenge".to_string());
        }
        let suffix = format!(".{}", dns_domain);
        let Some(host_part) = identifier.strip_suffix(&suffix) else {
            return Err("ACME 域名必须位于配置的 DNS 主域名下".to_string());
        };
        if host_part.trim().is_empty() {
            Ok("_acme-challenge".to_string())
        } else {
            Ok(format!("_acme-challenge.{}", host_part.trim_matches('.')))
        }
    }

    pub async fn upsert_txt_record(
        &self,
        rr: &str,
        value: &str,
        ttl: u32,
    ) -> Result<String, String> {
        let rr = rr.trim();
        let value = value.trim();
        if rr.is_empty() || value.is_empty() {
            return Err("TXT 记录名和值不能为空".to_string());
        }
        let sub_domain = format!("{}.{}", rr, self.config.domain.trim());
        let records = self.describe_sub_domain_records(&sub_domain, Some("TXT")).await?;
        if records
            .iter()
            .any(|record| record.rr == rr && record.record_type == "TXT" && record.value == value)
        {
            return Ok(format!("TXT 记录已存在：{} -> {}", sub_domain, value));
        }

        self.add_domain_record_with(&self.config.domain, rr, "TXT", value, ttl)
            .await?;
        Ok(format!("TXT 记录已添加：{} -> {}", sub_domain, value))
    }

    pub async fn delete_txt_record(&self, rr: &str, value: &str) -> Result<(), String> {
        let rr = rr.trim();
        let value = value.trim();
        if rr.is_empty() || value.is_empty() {
            return Ok(());
        }
        let sub_domain = format!("{}.{}", rr, self.config.domain.trim());
        let records = self.describe_sub_domain_records(&sub_domain, Some("TXT")).await?;
        for record in records
            .into_iter()
            .filter(|record| record.rr == rr && record.record_type == "TXT" && record.value == value)
        {
            self.delete_domain_record(&record.record_id).await?;
        }
        Ok(())
    }

    /// Test connectivity and credentials by querying DNS records.
    pub async fn test_connection(&self) -> Result<String, String> {
        if self.config.access_key_id.is_empty() || self.config.access_key_secret.is_empty() {
            return Err("AccessKey ID 或 Secret 未配置".to_string());
        }

        let sub_domain = Self::record_fqdn(&self.config);
        match self.describe_sub_domain_records(&sub_domain, None).await {
            Ok(records) => {
                add_log(
                    "info",
                    "DDNS",
                    &format!(
                        "DDNS 连接测试成功：域名 {} 当前有 {} 条记录",
                        sub_domain,
                        records.len()
                    ),
                );
                Ok(format!("连接成功，找到 {} 条 DNS 记录", records.len()))
            }
            Err(e) => {
                add_log("error", "DDNS", &format!("DDNS 连接测试失败：{}", e));
                Err(format!("连接测试失败：{}", e))
            }
        }
    }

    /// Fetch the current DNS record value for the configured subdomain.
    pub async fn describe_record(&self) -> Result<String, String> {
        let sub_domain = Self::record_fqdn(&self.config);
        let records = self
            .describe_sub_domain_records(&sub_domain, Some(&self.config.record_type))
            .await?;

        if records.is_empty() {
            Ok("未找到 DNS 记录".to_string())
        } else {
            Ok(records[0].value.clone())
        }
    }

    /// Update the DDNS record. If the record exists, update it; otherwise create it.
    /// Returns a human-readable result message.
    pub async fn update_record(&self, ipv4: &str, ipv6: &str) -> Result<String, String> {
        let target_ip = if self.config.record_type == "AAAA" {
            if ipv6.is_empty() {
                return Err("未获取到 IPv6 地址".to_string());
            }
            ipv6
        } else {
            if ipv4.is_empty() {
                return Err("未获取到 IPv4 地址".to_string());
            }
            ipv4
        };

        let sub_domain = Self::record_fqdn(&self.config);
        let rr = Self::record_rr(&self.config);

        // Query existing record
        let records = self
            .describe_sub_domain_records(&sub_domain, Some(&self.config.record_type))
            .await?;

        if let Some(record) = records.into_iter().find(|r| {
            r.rr == rr && r.record_type == self.config.record_type
        }) {
            if record.value == target_ip {
                add_log(
                    "info",
                    "DDNS",
                    &format!(
                        "DNS 记录无需更新：{} 当前为 {}",
                        sub_domain, target_ip
                    ),
                );
                return Ok(format!("无需更新，当前值已是 {}", target_ip));
            }
            self.update_domain_record(&record.record_id, target_ip).await?;
            add_log(
                "info",
                "DDNS",
                &format!(
                    "DNS 记录已更新：{} → {} (record_id={})",
                    sub_domain, target_ip, record.record_id
                ),
            );
            Ok(format!("已更新：{} → {}", sub_domain, target_ip))
        } else {
            self.add_domain_record(target_ip).await?;
            add_log(
                "info",
                "DDNS",
                &format!("DNS 记录已新增：{} → {}", sub_domain, target_ip),
            );
            Ok(format!("已新增：{} → {}", sub_domain, target_ip))
        }
    }

    // ---------- private helpers ----------

    async fn describe_sub_domain_records(
        &self,
        sub_domain: &str,
        record_type: Option<&str>,
    ) -> Result<Vec<DnsRecord>, String> {
        let mut params = BTreeMap::new();
        params.insert("Action".to_string(), "DescribeSubDomainRecords".to_string());
        params.insert("SubDomain".to_string(), sub_domain.to_string());
        if let Some(t) = record_type {
            params.insert("Type".to_string(), t.to_string());
        }

        let resp = self.call_api(params).await?;
        if let Some(code) = resp.code {
            return Err(format!(
                "API 错误: {} — {}",
                code,
                resp.message.unwrap_or_default()
            ));
        }
        Ok(resp
            .domain_records
            .map(|dr| dr.record)
            .unwrap_or_default())
    }

    async fn update_domain_record(&self, record_id: &str, value: &str) -> Result<(), String> {
        self.update_domain_record_with(
            record_id,
            &Self::record_rr(&self.config),
            &self.config.record_type,
            value,
            self.config.ttl,
        )
        .await
    }

    async fn update_domain_record_with(
        &self,
        record_id: &str,
        rr: &str,
        record_type: &str,
        value: &str,
        ttl: u32,
    ) -> Result<(), String> {
        let mut params = BTreeMap::new();
        params.insert("Action".to_string(), "UpdateDomainRecord".to_string());
        params.insert("RecordId".to_string(), record_id.to_string());
        params.insert("RR".to_string(), rr.to_string());
        params.insert("Type".to_string(), record_type.to_string());
        params.insert("Value".to_string(), value.to_string());
        params.insert("TTL".to_string(), ttl.to_string());

        let resp = self.call_api(params).await?;
        if let Some(code) = resp.code {
            return Err(format!(
                "更新记录失败: {} — {}",
                code,
                resp.message.unwrap_or_default()
            ));
        }
        Ok(())
    }

    async fn add_domain_record(&self, value: &str) -> Result<(), String> {
        self.add_domain_record_with(
            &self.config.domain,
            &Self::record_rr(&self.config),
            &self.config.record_type,
            value,
            self.config.ttl,
        )
        .await
    }

    async fn add_domain_record_with(
        &self,
        domain: &str,
        rr: &str,
        record_type: &str,
        value: &str,
        ttl: u32,
    ) -> Result<(), String> {
        let mut params = BTreeMap::new();
        params.insert("Action".to_string(), "AddDomainRecord".to_string());
        params.insert("DomainName".to_string(), domain.to_string());
        params.insert("RR".to_string(), rr.to_string());
        params.insert("Type".to_string(), record_type.to_string());
        params.insert("Value".to_string(), value.to_string());
        params.insert("TTL".to_string(), ttl.to_string());

        let resp = self.call_api(params).await?;
        if let Some(code) = resp.code {
            return Err(format!(
                "新增记录失败: {} — {}",
                code,
                resp.message.unwrap_or_default()
            ));
        }
        Ok(())
    }

    async fn delete_domain_record(&self, record_id: &str) -> Result<(), String> {
        let mut params = BTreeMap::new();
        params.insert("Action".to_string(), "DeleteDomainRecord".to_string());
        params.insert("RecordId".to_string(), record_id.to_string());

        let resp = self.call_api(params).await?;
        if let Some(code) = resp.code {
            return Err(format!(
                "删除记录失败: {} - {}",
                code,
                resp.message.unwrap_or_default()
            ));
        }
        Ok(())
    }

    async fn call_api(&self, mut params: BTreeMap<String, String>) -> Result<AliyunResponse, String> {
        // Common parameters
        params.insert("Format".to_string(), "JSON".to_string());
        params.insert("Version".to_string(), API_VERSION.to_string());
        params.insert("AccessKeyId".to_string(), self.config.access_key_id.clone());
        params.insert("SignatureMethod".to_string(), "HMAC-SHA256".to_string());
        params.insert("SignatureVersion".to_string(), "1.0".to_string());
        params.insert(
            "Timestamp".to_string(),
            Self::utc_timestamp(),
        );
        params.insert("SignatureNonce".to_string(), uuid::Uuid::new_v4().to_string());

        let signature = Self::sign(&params, &self.config.access_key_secret);
        params.insert("Signature".to_string(), signature);

        let url = Self::build_url(&params)?;

        let response = self
            .client
            .get(url)
            .send()
            .await
            .map_err(|e| format!("HTTP 请求失败: {}", e))?
            .json::<AliyunResponse>()
            .await
            .map_err(|e| format!("解析响应失败: {}", e))?;

        Ok(response)
    }

    // ---------- signature helpers (pure functions) ----------

    fn sign(params: &BTreeMap<String, String>, secret: &str) -> String {
        let canonical = Self::canonicalize(params);
        let string_to_sign = format!("GET&{}&{}", Self::percent_encode("/"), Self::percent_encode(&canonical));
        let key = format!("{}&", secret);

        let mut mac = HmacSha256::new_from_slice(key.as_bytes())
            .expect("HMAC can take key of any size");
        mac.update(string_to_sign.as_bytes());
        let result = mac.finalize();
        BASE64.encode(result.into_bytes())
    }

    fn canonicalize(params: &BTreeMap<String, String>) -> String {
        let mut parts: Vec<String> = params
            .iter()
            .map(|(k, v)| {
                format!(
                    "{}={}",
                    Self::percent_encode(k),
                    Self::percent_encode(v)
                )
            })
            .collect();
        parts.sort();
        parts.join("&")
    }

    fn percent_encode(s: &str) -> String {
        let mut result = String::with_capacity(s.len() * 3);
        for byte in s.bytes() {
            match byte {
                b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                    result.push(byte as char);
                }
                _ => {
                    result.push_str(&format!("%{:02X}", byte));
                }
            }
        }
        result
    }

    fn build_url(params: &BTreeMap<String, String>) -> Result<Url, String> {
        let query: Vec<String> = params
            .iter()
            .map(|(k, v)| {
                format!(
                    "{}={}",
                    Self::percent_encode_loose(k),
                    Self::percent_encode_loose(v)
                )
            })
            .collect();

        let url_str = format!("{}?{}", ENDPOINT, query.join("&"));
        Url::parse(&url_str).map_err(|e| format!("URL 构建失败: {}", e))
    }

    /// A looser encode that also encodes ':' and '/' for the final URL query string.
    fn percent_encode_loose(s: &str) -> String {
        let mut result = String::with_capacity(s.len() * 3);
        for byte in s.bytes() {
            match byte {
                b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                    result.push(byte as char);
                }
                _ => {
                    result.push_str(&format!("%{:02X}", byte));
                }
            }
        }
        result
    }

    fn utc_timestamp() -> String {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default();
        let secs = now.as_secs();
        // Format: 2024-01-01T12:00:00Z
        let dt = secs_to_utc(secs);
        format!(
            "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
            dt.0, dt.1, dt.2, dt.3, dt.4, dt.5
        )
    }
}

fn secs_to_utc(total_secs: u64) -> (u32, u32, u32, u32, u32, u32) {
    let days = total_secs / 86400;
    let mut year = 1970u32;
    let mut remaining_days = days as u32;

    loop {
        let days_in_year = if is_leap(year) { 366 } else { 365 };
        if remaining_days < days_in_year {
            break;
        }
        remaining_days -= days_in_year;
        year += 1;
    }

    let month_days = if is_leap(year) {
        [31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    } else {
        [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    };

    let mut month = 0u32;
    for (i, &md) in month_days.iter().enumerate() {
        if remaining_days < md {
            month = i as u32 + 1;
            break;
        }
        remaining_days -= md;
    }

    let day = remaining_days + 1;
    let remaining_secs = total_secs % 86400;
    let hour = (remaining_secs / 3600) as u32;
    let minute = ((remaining_secs % 3600) / 60) as u32;
    let second = (remaining_secs % 60) as u32;

    (year, month, day, hour, minute, second)
}

fn is_leap(year: u32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn acme_challenge_rr_uses_relative_host_under_dns_domain() {
        assert_eq!(
            AliyunDdns::acme_challenge_rr("proxy.example.com", "example.com"),
            Ok("_acme-challenge.proxy".to_string())
        );
        assert_eq!(
            AliyunDdns::acme_challenge_rr("example.com", "example.com"),
            Ok("_acme-challenge".to_string())
        );
    }

    #[test]
    fn acme_challenge_rr_strips_wildcard_prefix() {
        assert_eq!(
            AliyunDdns::acme_challenge_rr("*.example.com", "example.com"),
            Ok("_acme-challenge".to_string())
        );
    }

    #[test]
    fn acme_challenge_rr_rejects_domain_outside_dns_zone() {
        let error = AliyunDdns::acme_challenge_rr("proxy.other.com", "example.com")
            .expect_err("outside zone should be rejected");

        assert!(error.contains("DNS 主域名"));
    }

    #[test]
    fn empty_sub_domain_targets_root_domain_record() {
        let config = DdnsConfig {
            domain: "example.com".to_string(),
            sub_domain: "  ".to_string(),
            ..DdnsConfig::default()
        };

        assert_eq!(AliyunDdns::record_rr(&config), "@");
        assert_eq!(AliyunDdns::record_fqdn(&config), "example.com");
    }

    #[test]
    fn non_empty_sub_domain_targets_named_record() {
        let config = DdnsConfig {
            domain: "example.com".to_string(),
            sub_domain: "home".to_string(),
            ..DdnsConfig::default()
        };

        assert_eq!(AliyunDdns::record_rr(&config), "home");
        assert_eq!(AliyunDdns::record_fqdn(&config), "home.example.com");
    }
}
