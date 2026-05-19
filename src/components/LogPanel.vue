<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { RefreshCw, Trash2 } from "@lucide/vue";
import type { LogEntry } from "../types";

const logs = ref<LogEntry[]>([]);
const levelFilter = ref("全部级别");
const refreshing = ref(false);
let timer: ReturnType<typeof setInterval> | null = null;

const filteredLogs = computed(() => {
  if (levelFilter.value === "全部级别") return logs.value;
  return logs.value.filter((log) => log.level.toUpperCase() === levelFilter.value);
});

async function loadLogs() {
  refreshing.value = true;
  try {
    const data = await invoke<LogEntry[]>("get_recent_logs");
    logs.value = data.slice().reverse();
  } catch {
    logs.value = [];
  } finally {
    refreshing.value = false;
  }
}

async function clearLogs() {
  try {
    await invoke("clear_logs");
  } finally {
    logs.value = [];
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
    if (Number.isNaN(date.getTime())) return timeStr;
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
      <div class="toolbar">
        <button class="btn btn-secondary" type="button" @click="clearLogs">
          <Trash2 :size="13" :stroke-width="2.1" />
          清空
        </button>
        <select v-model="levelFilter" class="level-select">
          <option>全部级别</option>
          <option>INFO</option>
          <option>WARN</option>
          <option>ERROR</option>
        </select>
        <button class="btn btn-secondary" type="button" :disabled="refreshing" @click="loadLogs">
          <RefreshCw :class="{ spinning: refreshing }" :size="13" :stroke-width="2.1" />
          刷新
        </button>
      </div>
    </header>

    <div class="log-table" role="table" aria-label="最近日志">
      <div class="log-head" role="row">
        <span>时间</span>
        <span>级别</span>
        <span>模块</span>
        <span>消息</span>
      </div>
      <div v-if="filteredLogs.length === 0" class="empty-state">暂无日志记录</div>
      <div v-for="log in filteredLogs" v-else :key="log.id" class="log-row" role="row">
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
  border: 1px solid rgba(218, 226, 237, 0.95);
  border-radius: var(--radius-md, 8px);
  background: rgba(255, 255, 255, 0.96);
  box-shadow: var(--shadow-panel);
}

.panel-header {
  height: 38px;
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  padding: 0 14px 0 18px;
  border-bottom: 1px solid #e6edf5;
}

h2 {
  color: #111827;
  font-size: 13px;
  font-weight: 800;
}

.toolbar {
  display: flex;
  align-items: center;
  gap: 8px;
}

.btn {
  height: 25px;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  gap: 5px;
  padding: 0 10px;
  border-radius: 4px;
  border: 1px solid #d8e1ec;
  background: #ffffff;
  color: #4b5563;
  font-size: 11px;
  font-weight: 700;
  white-space: nowrap;
}

.btn:disabled {
  cursor: wait;
  opacity: 0.62;
}

.level-select {
  height: 25px;
  border: 1px solid #d8e1ec;
  border-radius: 4px;
  background: #ffffff;
  color: #4b5563;
  padding: 0 26px 0 10px;
  font-size: 11px;
  font-weight: 700;
  outline: 0;
}

.log-table {
  flex: 1 1 auto;
  min-height: 0;
  overflow: auto;
  scrollbar-gutter: stable;
  scrollbar-width: thin;
  scrollbar-color: #b8c7d8 #f3f7fc;
}

.log-head,
.log-row {
  min-width: 1120px;
  display: grid;
  grid-template-columns: 168px 70px 86px minmax(0, 1fr);
  align-items: center;
  gap: 12px;
  min-height: 23px;
  padding: 0 18px;
  border-bottom: 1px solid #e6edf5;
  font-size: 11px;
}

.log-head {
  position: sticky;
  top: 0;
  z-index: 2;
  height: 27px;
  color: #182033;
  font-weight: 800;
  background: #fbfcfe;
}

.log-table::-webkit-scrollbar {
  width: 8px;
  height: 8px;
}

.log-table::-webkit-scrollbar-track {
  background: #f3f7fc;
}

.log-table::-webkit-scrollbar-thumb {
  border: 2px solid #f3f7fc;
  border-radius: 999px;
  background: #b8c7d8;
}

.log-time {
  color: #303847;
  font-variant-numeric: tabular-nums;
  white-space: nowrap;
}

.log-level {
  width: 43px;
  height: 18px;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  border-radius: 4px;
  font-size: 10px;
  font-weight: 800;
}

.level-info {
  color: #1769f6;
}

.level-warn {
  color: #d97706;
}

.level-error {
  color: #dc2626;
}

.log-module {
  color: #303847;
  font-weight: 700;
}

.log-message {
  min-width: 0;
  color: #303847;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.empty-state {
  height: 108px;
  display: grid;
  place-items: center;
  color: #7b8495;
  font-size: 12px;
  font-weight: 700;
}

.spinning {
  animation: spin 0.8s linear infinite;
}

@keyframes spin {
  to {
    transform: rotate(360deg);
  }
}
</style>
