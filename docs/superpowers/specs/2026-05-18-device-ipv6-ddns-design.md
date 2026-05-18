# 局域网设备 IPv6 DDNS 设计

## 背景

HomeNet 当前已有阿里云 DDNS、IPv6 网卡绑定、运行状态卡片、转发规则和日志面板。现有 DDNS 面向当前运行 HomeNet 的主机，转发规则面板已经形成独立工作流。

本次目标采用方案 A：为局域网内某一台具备公网 IPv6 的设备单独配置 DDNS，使子域名的 AAAA 记录指向该设备的公网 IPv6。

## 目标

- 获取局域网设备列表，展示设备名称、IPv4、IPv6、MAC、在线状态和最近发现时间。
- 允许用户选择一台具备公网 IPv6 的设备，并为其绑定一个子域名。
- 定时或手动更新阿里云 DNS 的 AAAA 记录到该设备的公网 IPv6。
- 保持现有转发规则功能和界面不变。
- 在界面上将设备级 DDNS 放到 DDNS 管理区，不侵入转发规则面板。

## 非目标

- 不为内网 IPv4 设备实现公网穿透。
- 不自动修改路由器防火墙、IPv6 入站规则或端口转发规则。
- 不扫描完整 IPv6 /64 地址空间。
- 不改变当前转发规则表格、编辑器、批量操作和协议能力。

## 布局设计

主界面仍保持三段式结构：

```text
状态卡片
DDNS 管理区 | 转发规则
日志
```

右侧 `ForwardRulesPanel` 保持现状。左侧原 `DdnsPanel` 扩展为 DDNS 管理区，内部包含两个区域：

```text
局域网设备
- 刷新设备
- 设备列表
- 在线状态
- 公网 IPv6 标记
- 选中设备的 IPv6

DDNS 配置
- 阿里云 AccessKey
- 主域名 / 子域名
- TTL / 更新间隔
- 当前解析值
- 测试连接 / 保存配置 / 立即更新
```

设备列表只作为 DDNS 目标选择器使用，不影响端口转发规则的输入方式。

## 设备发现设计

新增后端模块 `device_discovery`，通过多来源合并生成设备列表：

- 优先读取系统邻居表：Windows 可使用 IPv6 邻居表和 ARP 表，获取 IP、MAC、状态。
- 对当前网卡所在 IPv4 网段做轻量探测，用于补全设备在线状态和 MAC 信息。
- 对 IPv6 使用本地链路多播探测或邻居表刷新，不暴力扫描 /64。
- 后续可扩展 mDNS、SSDP、NetBIOS 主机名解析。

设备发现结果以运行时数据为主，不把扫描结果直接持久化为配置。绑定 DDNS 时只持久化用户明确选择的设备标识和域名配置。

## 数据模型

新增运行时设备结构：

```text
LanDevice
- id
- display_name
- hostname
- mac
- ipv4[]
- ipv6[]
- global_ipv6[]
- online
- source
- last_seen
```

新增设备 DDNS 绑定结构：

```text
DeviceDdnsBinding
- id
- enabled
- provider
- access_key_id
- access_key_secret
- domain
- sub_domain
- ttl
- interval_minutes
- device_id
- device_mac
- device_name
- selected_ipv6
- last_update_time
- last_result
```

`selected_ipv6` 每次更新前重新从设备列表中匹配，优先使用同一 MAC 或设备标识对应的公网 IPv6。若设备 IPv6 变化，自动更新 AAAA 记录。

## DDNS 更新流程

手动更新：

```text
用户选择设备 -> 校验设备有公网 IPv6 -> 校验阿里云配置 -> 查询现有 AAAA -> 更新或新增记录 -> 写日志 -> 刷新当前解析值
```

定时更新：

```text
定时器触发 -> 重新发现设备 -> 匹配绑定设备 -> 获取公网 IPv6 -> 更新 AAAA -> 写入状态
```

如果设备离线、没有公网 IPv6、凭据无效或阿里云 API 失败，界面显示错误状态并写入日志，不改写 DNS 记录。

## 错误处理

- 未发现设备：显示空状态，允许手动刷新。
- 设备无公网 IPv6：禁止立即绑定，提示该设备不适合方案 A。
- 设备离线：保留绑定配置，但跳过更新。
- 设备 IPv6 变化：用新公网 IPv6 更新 AAAA。
- 多个公网 IPv6：默认选择第一个稳定公网 IPv6，界面允许用户切换。
- DNS 记录已是目标值：显示无需更新。
- AccessKey 缺失或失败：沿用当前 DDNS 错误提示和日志机制。

## 测试策略

- 单元测试公网 IPv6 判断、设备去重、设备绑定匹配和 DDNS 更新决策。
- 后端命令测试设备列表为空、设备无公网 IPv6、设备 IPv6 变化等场景。
- 前端验证 DDNS 管理区布局、设备选择、按钮禁用状态和错误提示。
- 回归验证 `ForwardRulesPanel` 的功能和视觉结构不变。

## 实施边界

第一阶段只实现一个设备级 DDNS 绑定，避免过早引入多设备批量管理。

后续可扩展为多个设备绑定，每个设备对应一个子域名，但该能力不进入第一阶段范围。
