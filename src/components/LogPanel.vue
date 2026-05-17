<script setup lang="ts">
import { onMounted, onUnmounted, ref } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { Trash2 } from "@lucide/vue";
import type { LogEntry } from "../types";

const logs = ref<LogEntry[]>([]);
let timer: ReturnType<typeof setInterval> | null = null;

async function loadLogs() {
  try {
    const data = await invoke<LogEntry[]>("get_recent_logs");
    logs.value = data.slice().reverse();
  } catch {
    logs.value = [];
  }
}

async function clearLogs() {
  try {
    await invoke("clear_logs");
    await loadLogs();
  } catch (e: any) {
    console.warn("清空日志失败:", e);
  }
}

function startAutoRefresh() {
  stopAutoRefresh();
  timer = setInterval(loadLogs, 5000);
}

function stopAutoRefresh() {
  if (timer !== null) {
    clearInterval(timer);
    timer = null;
  }
}

const levelClass = (level: string): string => {
  switch (level.toUpperCase()) {
    case "INFO":
      return "level-info";
    case "WARN":
    case "WARNING":
      return "level-warn";
    case "ERROR":
      return "level-error";
    default:
      return "level-info";
  }
};

function formatTime(timeStr: string): string {
  try {
    const date = new Date(timeStr);
    if (isNaN(date.getTime())) return timeStr;
    const pad = (n: number) => String(n).padStart(2, "0");
    return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())} ${pad(
      date.getHours(),
    )}:${pad(date.getMinutes())}:${pad(date.getSeconds())}`;
  } catch {
    return timeStr;
  }
}

onMounted(() => {
  loadLogs();
  startAutoRefresh();
  window.addEventListener("homenet:logs-refresh", loadLogs);
});

onUnmounted(() => {
  stopAutoRefresh();
  window.removeEventListener("homenet:logs-refresh", loadLogs);
});
</script>

<template>
  <section class="panel log-panel">
    <header class="panel-header">
      <h2>最近日志</h2>
      <button class="clear-button" type="button" @click="clearLogs">
        <Trash2 :size="15" :stroke-width="2.1" />
        清空
      </button>
    </header>

    <div v-if="logs.length === 0" class="empty-state">
      暂无日志记录
    </div>

    <div v-else class="log-table" role="table" aria-label="最近日志">
      <div v-for="log in logs" :key="log.id" class="log-row" role="row">
        <time class="log-time">{{ formatTime(log.time) }}</time>
        <span class="log-level" :class="levelClass(log.level)">
          {{ log.level.toUpperCase() }}
        </span>
        <span class="log-module">{{ log.module }}</span>
        <span class="log-message">{{ log.message }}</span>
      </div>
    </div>
  </section>
</template>

<style scoped>
.panel {
  min-width: 0;
  min-height: 0;
  display: flex;
  flex-direction: column;
  overflow: hidden;
  border: 1px solid rgba(217, 225, 237, 0.95);
  border-radius: var(--radius-md, 8px);
  background: rgba(255, 255, 255, 0.94);
  box-shadow: var(--shadow-card);
}

.panel-header {
  height: 46px;
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 0 18px;
  border-bottom: 1px solid #e1e8f2;
}

.panel-header h2 {
  font-size: 16px;
  font-weight: 800;
  color: #151922;
}

.clear-button {
  height: 30px;
  display: inline-flex;
  align-items: center;
  gap: 7px;
  padding: 0 12px;
  border: 1px solid #d7e0eb;
  border-radius: 5px;
  background: #ffffff;
  color: #525d6b;
  font-size: 12px;
  font-weight: 700;
}

.clear-button svg {
  display: block;
}

.trash-icon {
  position: relative;
  width: 15px;
  height: 15px;
  display: inline-block;
}

.trash-icon::before,
.trash-icon::after {
  content: "";
  position: absolute;
  display: block;
}

.trash-icon::before {
  left: 4px;
  top: 5px;
  width: 8px;
  height: 9px;
  border: 1.7px solid currentColor;
  border-top: 0;
  border-radius: 0 0 2px 2px;
}

.trash-icon::after {
  left: 3px;
  top: 2px;
  width: 10px;
  height: 2px;
  background: currentColor;
  box-shadow: 3px -2px 0 -0.5px currentColor;
}

.empty-state {
  display: grid;
  place-items: center;
  min-height: 132px;
  color: #8a94a6;
}

.log-table {
  min-height: 0;
  overflow-y: auto;
}

.log-row {
  min-height: 34px;
  display: grid;
  grid-template-columns: 168px 70px 70px minmax(0, 1fr);
  align-items: center;
  gap: 8px;
  padding: 0 18px;
  border-bottom: 1px solid #e6edf5;
  font-size: 12px;
}

.log-row:last-child {
  border-bottom: 0;
}

.log-time {
  color: #4b5563;
  font-variant-numeric: tabular-nums;
  white-space: nowrap;
}

.log-level {
  width: 50px;
  height: 21px;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  border-radius: 999px;
  font-size: 11px;
  font-weight: 800;
}

.level-info {
  color: #2563eb;
  background: #eaf2ff;
  border: 1px solid #cfe0ff;
}

.level-warn {
  color: #d97706;
  background: #fff6df;
  border: 1px solid #f7dfaa;
}

.level-error {
  color: #dc2626;
  background: #fee2e2;
  border: 1px solid #fecaca;
}

.log-module {
  color: #4b5563;
  font-weight: 700;
}

.log-message {
  min-width: 0;
  color: #303746;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}
</style>
