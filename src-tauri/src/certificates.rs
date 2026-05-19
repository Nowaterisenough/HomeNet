use crate::config::{add_log, DdnsConfig, ReverseProxyRule};
use crate::ddns::aliyun::AliyunDdns;
use chrono::{DateTime, Duration, Utc};
use instant_acme::{
    Account, AccountCredentials, AuthorizationStatus, ChallengeType, Identifier, NewAccount,
    NewOrder, OrderStatus, RetryPolicy,
};
use std::collections::hash_map::DefaultHasher;
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::time::Duration as StdDuration;

const CERT_RENEW_AFTER_DAYS: i64 = 60;
const CERT_VALIDITY_HINT_DAYS: i64 = 90;
const DNS_PROPAGATION_WAIT_SECS: u64 = 30;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CertificatePaths {
    pub cert_path: PathBuf,
    pub key_path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IssuedCertificate {
    pub cert_path: PathBuf,
    pub key_path: PathBuf,
    pub issued_at: String,
    pub expires_at: String,
}

fn certificates_base_dir() -> PathBuf {
    let base = dirs_next::config_dir().unwrap_or_else(|| PathBuf::from("."));
    base.join("home-net").join("certificates")
}

fn sanitize_domain_for_path(domain: &str) -> Result<String, String> {
    let mut normalized = domain
        .trim()
        .trim_end_matches('.')
        .to_ascii_lowercase()
        .replace("*.", "wildcard.");
    normalized = normalized
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '.' || ch == '-' || ch == '_' {
                ch
            } else {
                '-'
            }
        })
        .collect::<String>()
        .trim_matches(['.', '-'])
        .to_string();
    if normalized.is_empty() {
        return Err("证书域名不能为空".to_string());
    }
    Ok(normalized)
}

pub fn certificate_file_paths(base_dir: &Path, domain: &str) -> Result<CertificatePaths, String> {
    let domain_dir = base_dir.join(sanitize_domain_for_path(domain)?);
    Ok(CertificatePaths {
        cert_path: domain_dir.join("fullchain.pem"),
        key_path: domain_dir.join("private-key.pem"),
    })
}

pub fn default_certificate_file_paths(domain: &str) -> Result<CertificatePaths, String> {
    certificate_file_paths(&certificates_base_dir(), domain)
}

pub fn certificate_renewal_due(rule: &ReverseProxyRule) -> bool {
    if !rule.tls.trim().eq_ignore_ascii_case("auto") {
        return false;
    }
    if rule.certificate_path.trim().is_empty() || rule.private_key_path.trim().is_empty() {
        return true;
    }
    if !Path::new(rule.certificate_path.trim()).exists()
        || !Path::new(rule.private_key_path.trim()).exists()
    {
        return true;
    }
    certificate_renewal_due_at(rule, &Utc::now().to_rfc3339())
}

pub fn certificate_renewal_due_at(rule: &ReverseProxyRule, now: &str) -> bool {
    if !rule.tls.trim().eq_ignore_ascii_case("auto") {
        return false;
    }
    if rule.certificate_path.trim().is_empty() || rule.private_key_path.trim().is_empty() {
        return true;
    }
    let Ok(issued_at) = DateTime::parse_from_rfc3339(rule.certificate_last_issued_at.trim()) else {
        return true;
    };
    let Ok(now) = DateTime::parse_from_rfc3339(now.trim()) else {
        return true;
    };
    now.signed_duration_since(issued_at) >= Duration::days(CERT_RENEW_AFTER_DAYS)
}

fn account_credentials_path(email: &str, directory_url: &str) -> PathBuf {
    let mut hasher = DefaultHasher::new();
    directory_url.hash(&mut hasher);
    let directory_hash = hasher.finish();
    let safe_email = email
        .trim()
        .to_ascii_lowercase()
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '.' || ch == '-' || ch == '_' {
                ch
            } else {
                '-'
            }
        })
        .collect::<String>();
    certificates_base_dir()
        .join("accounts")
        .join(format!("{}-{:016x}.json", safe_email, directory_hash))
}

async fn load_or_create_account(
    email: &str,
    directory_url: &str,
) -> Result<Account, String> {
    let credentials_path = account_credentials_path(email, directory_url);
    if let Ok(content) = fs::read_to_string(&credentials_path) {
        let credentials: AccountCredentials = serde_json::from_str(&content)
            .map_err(|e| format!("解析 ACME 账号凭据失败: {}", e))?;
        return Account::builder()
            .map_err(|e| format!("创建 ACME 客户端失败: {}", e))?
            .from_credentials(credentials)
            .await
            .map_err(|e| format!("加载 ACME 账号失败: {}", e));
    }

    let contact_uri = format!("mailto:{}", email.trim());
    let contacts = [contact_uri.as_str()];
    let (account, credentials) = Account::builder()
        .map_err(|e| format!("创建 ACME 客户端失败: {}", e))?
        .create(
            &NewAccount {
                contact: &contacts,
                terms_of_service_agreed: true,
                only_return_existing: false,
            },
            directory_url.to_string(),
            None,
        )
        .await
        .map_err(|e| format!("创建 ACME 账号失败: {}", e))?;

    if let Some(parent) = credentials_path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("创建 ACME 账号目录失败: {}", e))?;
    }
    let content = serde_json::to_string_pretty(&credentials)
        .map_err(|e| format!("序列化 ACME 账号凭据失败: {}", e))?;
    fs::write(&credentials_path, content).map_err(|e| format!("保存 ACME 账号凭据失败: {}", e))?;
    Ok(account)
}

fn acme_dns_config(rule: &ReverseProxyRule) -> DdnsConfig {
    DdnsConfig {
        enabled: true,
        provider: rule.acme_dns_provider.clone(),
        access_key_id: rule.acme_access_key_id.clone(),
        access_key_secret: rule.acme_access_key_secret.clone(),
        domain: rule.acme_dns_domain.clone(),
        sub_domain: String::new(),
        record_type: "TXT".to_string(),
        ttl: 600,
        interval_minutes: 10,
    }
}

fn write_certificate_files(
    paths: &CertificatePaths,
    cert_chain_pem: &str,
    private_key_pem: &str,
) -> Result<(), String> {
    if let Some(parent) = paths.cert_path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("创建证书目录失败: {}", e))?;
    }
    fs::write(&paths.cert_path, cert_chain_pem)
        .map_err(|e| format!("写入证书文件失败: {}", e))?;
    fs::write(&paths.key_path, private_key_pem).map_err(|e| format!("写入私钥文件失败: {}", e))?;
    Ok(())
}

pub async fn issue_certificate(rule: &ReverseProxyRule) -> Result<IssuedCertificate, String> {
    if !rule.tls.trim().eq_ignore_ascii_case("auto") {
        return Err("当前反代规则未启用 ACME 自动证书".to_string());
    }

    let domain = rule.domain.trim().trim_end_matches('.').to_ascii_lowercase();
    let paths = default_certificate_file_paths(&domain)?;
    let dns = AliyunDdns::new(acme_dns_config(rule));
    let account =
        load_or_create_account(rule.acme_email.trim(), rule.acme_directory_url.trim()).await?;
    let identifiers = [Identifier::Dns(domain.clone())];
    let mut order = account
        .new_order(&NewOrder::new(&identifiers))
        .await
        .map_err(|e| format!("创建 ACME 订单失败: {}", e))?;

    let mut cleanup_records = Vec::<(String, String)>::new();
    let result = async {
        let mut authorizations = order.authorizations();
        while let Some(result) = authorizations.next().await {
            let mut authz = result.map_err(|e| format!("读取 ACME 授权失败: {}", e))?;
            match authz.status {
                AuthorizationStatus::Pending => {}
                AuthorizationStatus::Valid => continue,
                _ => return Err(format!("ACME 授权状态不可用: {:?}", authz.status)),
            }

            let mut challenge = authz
                .challenge(ChallengeType::Dns01)
                .ok_or_else(|| "ACME 服务未返回 DNS-01 挑战".to_string())?;
            let identifier = challenge.identifier().to_string();
            let rr = AliyunDdns::acme_challenge_rr(&identifier, &rule.acme_dns_domain)?;
            let value = challenge.key_authorization().dns_value();
            dns.upsert_txt_record(&rr, &value, 600).await?;
            cleanup_records.push((rr, value));
            tokio::time::sleep(StdDuration::from_secs(DNS_PROPAGATION_WAIT_SECS)).await;
            challenge
                .set_ready()
                .await
                .map_err(|e| format!("提交 ACME DNS-01 挑战失败: {}", e))?;
        }

        let status = order
            .poll_ready(&RetryPolicy::default())
            .await
            .map_err(|e| format!("等待 ACME 订单就绪失败: {}", e))?;
        if status != OrderStatus::Ready {
            return Err(format!("ACME 订单未就绪: {:?}", status));
        }

        let private_key_pem = order
            .finalize()
            .await
            .map_err(|e| format!("生成 ACME 证书私钥失败: {}", e))?;
        let cert_chain_pem = order
            .poll_certificate(&RetryPolicy::default())
            .await
            .map_err(|e| format!("下载 ACME 证书失败: {}", e))?;

        write_certificate_files(&paths, &cert_chain_pem, &private_key_pem)?;
        let issued_at = Utc::now();
        Ok(IssuedCertificate {
            cert_path: paths.cert_path.clone(),
            key_path: paths.key_path.clone(),
            issued_at: issued_at.to_rfc3339(),
            expires_at: (issued_at + Duration::days(CERT_VALIDITY_HINT_DAYS)).to_rfc3339(),
        })
    }
    .await;

    for (rr, value) in cleanup_records {
        if let Err(error) = dns.delete_txt_record(&rr, &value).await {
            add_log(
                "warn",
                "证书",
                &format!("清理 ACME TXT 记录失败：{} -> {}，{}", rr, value, error),
            );
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ReverseProxyRule;
    use std::path::Path;

    #[test]
    fn certificate_file_paths_are_domain_scoped_and_safe() {
        let paths = certificate_file_paths(Path::new("C:/home-net/certificates"), "*.Proxy.Example.COM")
            .expect("domain should produce paths");

        assert!(paths.cert_path.ends_with("wildcard.proxy.example.com/fullchain.pem"));
        assert!(paths.key_path.ends_with("wildcard.proxy.example.com/private-key.pem"));
    }

    #[test]
    fn auto_certificate_rule_needs_issue_when_files_are_missing() {
        let rule = ReverseProxyRule {
            protocol: "HTTPS".to_string(),
            tls: "auto".to_string(),
            certificate_path: String::new(),
            private_key_path: String::new(),
            ..ReverseProxyRule::default()
        };

        assert!(certificate_renewal_due(&rule));
    }

    #[test]
    fn auto_certificate_rule_renews_after_sixty_days() {
        let mut rule = ReverseProxyRule {
            protocol: "HTTPS".to_string(),
            tls: "auto".to_string(),
            certificate_path: "cert.pem".to_string(),
            private_key_path: "key.pem".to_string(),
            certificate_last_issued_at: "2026-01-01T00:00:00Z".to_string(),
            ..ReverseProxyRule::default()
        };

        assert!(!certificate_renewal_due_at(&rule, "2026-02-15T00:00:00Z"));
        assert!(certificate_renewal_due_at(&rule, "2026-03-03T00:00:00Z"));

        rule.tls = "passthrough".to_string();
        assert!(!certificate_renewal_due_at(&rule, "2026-03-03T00:00:00Z"));
    }
}
