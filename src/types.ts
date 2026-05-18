export interface AppConfig {
  version: string;
  auto_start: boolean;
  start_minimized: boolean;
  log_level: string;
  ipv6_interface: string;
  ddns: DdnsConfig;
  forward_rules: ForwardRule[];
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
  public_ipv4: string;
  public_ipv6: string;
  ddns_status: string;
  last_update_time: string;
  rule_count: number;
  enabled_rule_count: number;
  uptime: number;
}

export interface AppUpdateResult {
  status: "up_to_date" | "installed" | string;
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
  ttl: number;
  interval_minutes: number;
  device_id: string;
  device_mac: string;
  device_name: string;
  selected_ipv6: string;
  last_update_time: string;
  last_result: string;
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
