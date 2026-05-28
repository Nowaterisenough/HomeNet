export interface AppConfig {
  version: string;
  auto_start: boolean;
  start_minimized: boolean;
  log_level: string;
  ipv6_interface: string;
  ddns: DdnsConfig;
  device_ddns: DeviceDdnsConfig;
  device_ddns_configs: DeviceDdnsConfig[];
  forward_rules: ForwardRule[];
  reverse_proxy_rules: ReverseProxyRule[];
}

export interface DdnsConfig {
  enabled: boolean;
  provider: string;
  access_key_id: string;
  access_key_secret: string;
  domain: string;
  sub_domain: string;
  record_type: string;
  ttl: number;
  interval_minutes: number;
}

export interface ForwardRule {
  id: string;
  enabled: boolean;
  protocol: string;
  listen_addr: string;
  listen_port: number;
  target_ip: string;
  target_port: number;
  mode: string;
  remark: string;
  status: string;
}

export interface RuntimeStatus {
  version: string;
  public_ipv4: string;
  public_ipv6: string;
  ddns_status: string;
  last_update_time: string;
  rule_count: number;
  enabled_rule_count: number;
  reverse_proxy_rule_count: number;
  enabled_reverse_proxy_rule_count: number;
  uptime: number;
}

export interface ReverseProxyRule {
  id: string;
  enabled: boolean;
  protocol: "HTTP" | "HTTPS" | string;
  domain: string;
  listen_addr: string;
  listen_port: number;
  backend_ip: string;
  backend_port: number;
  tls: string;
  certificate: string;
  acme_email: string;
  acme_dns_provider: string;
  acme_access_key_id: string;
  acme_access_key_secret: string;
  acme_dns_domain: string;
  acme_directory_url: string;
  certificate_path: string;
  private_key_path: string;
  certificate_expires_at: string;
  certificate_last_issued_at: string;
  certificate_last_error: string;
  remark: string;
  status: string;
}

export interface AppUpdateResult {
  status: "up_to_date" | "installed" | "unavailable" | string;
  current_version: string;
  latest_version: string;
  message: string;
}

export interface NetworkInterfaceInfo {
  name: string;
  ipv4: string[];
  ipv6: string[];
  has_global_ipv6: boolean;
}

export interface LogEntry {
  id: string;
  time: string;
  level: string;
  module: string;
  message: string;
}

export interface DeviceDdnsConfig {
  enabled: boolean;
  provider: string;
  access_key_id: string;
  access_key_secret: string;
  domain: string;
  sub_domain: string;
  record_type: string;
  ttl: number;
  interval_minutes: number;
  device_id: string;
  device_mac: string;
  device_name: string;
  selected_ipv6: string;
  selected_ip: string;
  last_update_time: string;
  last_result: string;
  last_online: boolean;
}

export interface LanDevice {
  id: string;
  display_name: string;
  hostname: string;
  mac: string;
  ipv4: string[];
  ipv6: string[];
  global_ipv6: string[];
  online: boolean;
  source: string;
  last_seen: string;
}
