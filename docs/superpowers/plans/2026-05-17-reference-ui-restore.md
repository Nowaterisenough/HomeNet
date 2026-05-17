# Reference UI Restore Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Recreate the supplied desktop UI screenshot while keeping the current Tauri-backed DDNS, forwarding-rule, and log functionality.

**Architecture:** Keep the existing Vue single-page component boundaries and replace corrupted text plus styling in place. `App.vue` owns shell layout, status fallback data, and runtime-status loading; child components keep their current Tauri command responsibilities.

**Tech Stack:** Vue 3 `<script setup>`, TypeScript, Vite, Tauri invoke API, scoped CSS.

---

## File Structure

- Modify `src/App.vue`: app shell, title bar, status card data, global tokens, desktop layout.
- Modify `src/components/Sidebar.vue`: light sidebar, navigation labels, bottom service block.
- Modify `src/components/StatusCard.vue`: reference-style status card with CSS icons.
- Modify `src/components/DdnsPanel.vue`: corrected Chinese labels, reference panel layout, functional controls.
- Modify `src/components/ForwardRulesPanel.vue`: fallback rules, reference table/editor layout, functional controls.
- Modify `src/components/LogPanel.vue`: fallback logs, reference compact table layout, functional controls.
- Keep `src/types.ts` unchanged.

## Tasks

### Task 1: Shell And Status Cards

**Files:**
- Modify: `src/App.vue`
- Modify: `src/components/StatusCard.vue`

- [ ] **Step 1: Add status fallback and runtime load**

Use fallback values matching the screenshot:

```ts
const statusData = ref<RuntimeStatus>({
  public_ipv4: "101.42.16.88",
  public_ipv6: "2408:4007:808:1234::1",
  ddns_status: "运行中",
  last_update_time: "2 分钟前",
  rule_count: 4,
  enabled_rule_count: 4,
  online_device_count: 8,
  uptime: 26,
});
```

Call `invoke<RuntimeStatus>("get_runtime_status")` on mount and merge non-empty values into the fallback.

- [ ] **Step 2: Rebuild the shell template**

Add a fixed top title bar and keep the existing sidebar/main regions:

```vue
<div class="window-frame">
  <header class="titlebar">...</header>
  <div class="app-shell">...</div>
</div>
```

- [ ] **Step 3: Restyle global variables**

Use pale gray background, white surfaces, blue primary, green success, 8px radii, and compact desktop spacing.

- [ ] **Step 4: Rewrite `StatusCard.vue`**

Accept the existing props and render icon discs with CSS classes instead of emoji icons. Keep `title`, `value`, and `subtitle` as text-driven props.

### Task 2: Sidebar

**Files:**
- Modify: `src/components/Sidebar.vue`

- [ ] **Step 1: Replace corrupted labels**

Use these nav items:

```ts
const navItems = [
  { key: "overview", label: "概览", icon: "home" },
  { key: "ddns", label: "阿里云 DDNS", icon: "cloud" },
  { key: "forward", label: "IPv6/IPv4 转发", icon: "nodes" },
  { key: "logs", label: "日志", icon: "file" },
  { key: "settings", label: "设置", icon: "gear" },
];
```

- [ ] **Step 2: Add service footer**

Render running status, version `1.0.3`, and an auto-start toggle.

- [ ] **Step 3: Restyle for reference**

Use width `240px`, light translucent background, selected row tint, CSS icons, and compact spacing.

### Task 3: DDNS Panel

**Files:**
- Modify: `src/components/DdnsPanel.vue`

- [ ] **Step 1: Keep real command calls**

Retain `get_ddns_config`, `save_ddns_config`, `test_ddns_connection`, and `trigger_ddns_update`.

- [ ] **Step 2: Seed reference fallback config**

Use enabled Aliyun values from the screenshot when the backend has empty config.

- [ ] **Step 3: Rewrite labels and footer**

Use Chinese labels from the screenshot and add footer text:

```text
最后成功更新：2025-05-15 14:32:18
查看历史日志
```

- [ ] **Step 4: Restyle panel**

Match compact two-column form layout inside the left panel, with primary/secondary action buttons.

### Task 4: Forward Rules Panel

**Files:**
- Modify: `src/components/ForwardRulesPanel.vue`

- [ ] **Step 1: Keep real command calls**

Retain list/save/delete/enable commands.

- [ ] **Step 2: Seed fallback rules**

Use the four screenshot rules: remote desktop, HTTPS, Web, SSH.

- [ ] **Step 3: Render screenshot table columns**

Columns must be select, enabled, protocol, listen address, listen port, target IP, target port, remark, status, actions.

- [ ] **Step 4: Keep inline editor visible for the selected first row**

Default `showEditor` to true and seed editor with the first fallback rule until real data is loaded.

- [ ] **Step 5: Restyle table, toolbar, toggles, editor**

Use blue selected row, green status pill, toolbar buttons, and compact fields matching the reference.

### Task 5: Logs Panel

**Files:**
- Modify: `src/components/LogPanel.vue`

- [ ] **Step 1: Keep real command calls**

Retain `get_recent_logs` and `clear_logs`.

- [ ] **Step 2: Seed fallback logs**

Use the five screenshot log rows.

- [ ] **Step 3: Restyle as compact table**

Render timestamp, level pill, module, and message in rows with a clear button in the header.

### Task 6: Verification

**Files:**
- No production edits expected.

- [ ] **Step 1: Run build**

Run:

```powershell
$nodeDir = Join-Path $env:APPDATA 'fnm\node-versions\v22.22.3\installation'
$cargoDir = Join-Path $env:USERPROFILE '.cargo\bin'
$env:PATH = "$nodeDir;$cargoDir;$env:PATH"
& "$nodeDir\pnpm.cmd" build
```

Expected: `vue-tsc --noEmit && vite build` exits 0.

- [ ] **Step 2: Verify dev server**

Check `http://localhost:1420` returns 200.

- [ ] **Step 3: Visually inspect desktop**

Compare the app to the reference at 1440x900. Confirm title bar, sidebar, cards, DDNS panel, rules panel/editor, and logs are present.

## Self-Review

- Spec coverage: all approved visual and functional requirements map to Tasks 1-6.
- Placeholder scan: no TBD/TODO placeholders remain.
- Type consistency: existing `RuntimeStatus`, `DdnsConfig`, `ForwardRule`, and `LogEntry` interfaces are reused.
