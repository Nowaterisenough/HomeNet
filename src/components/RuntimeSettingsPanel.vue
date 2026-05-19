<script setup lang="ts">
import { computed } from "vue";
import { RefreshCw } from "@lucide/vue";

const props = defineProps<{
  uptime: number;
  version: string;
  autoStartEnabled: boolean;
  autoStartSaving: boolean;
  updateChecking: boolean;
}>();

const emit = defineEmits<{
  "toggle-autostart": [enabled: boolean];
  "check-update": [];
}>();

const formattedUptime = computed(() => {
  const totalSeconds = Math.max(0, Math.floor(Number(props.uptime) || 0));
  const minutes = Math.floor(totalSeconds / 60);
  const hours = Math.floor(minutes / 60);
  const days = Math.floor(hours / 24);

  if (totalSeconds < 60) return `${totalSeconds} 秒`;
  if (minutes < 60) return `${minutes} 分钟`;
  if (hours < 24) return `${hours} 小时 ${minutes % 60} 分钟`;
  return `${days} 天 ${hours % 24} 小时`;
});

function onAutoStartChange(event: Event) {
  const input = event.target as HTMLInputElement;
  emit("toggle-autostart", input.checked);
}
</script>

<template>
  <aside class="runtime-panel">
    <h2>运行时与设置</h2>

    <dl class="runtime-list">
      <div>
        <dt>运行时长</dt>
        <dd>{{ formattedUptime }}</dd>
      </div>
      <div>
        <dt>版本信息</dt>
        <dd>{{ version }}</dd>
      </div>
      <div class="update-row">
        <dt>检查更新</dt>
        <dd>
          <button
            class="check-button"
            type="button"
            :disabled="updateChecking"
            @click="emit('check-update')"
          >
            <RefreshCw
              :class="{ spinning: updateChecking }"
              :size="13"
              :stroke-width="2.2"
            />
            检查更新
          </button>
        </dd>
      </div>
    </dl>

    <label class="setting-toggle">
      <span>开机自动启动</span>
      <input
        type="checkbox"
        :checked="autoStartEnabled"
        :disabled="autoStartSaving"
        @change="onAutoStartChange"
      />
    </label>
    <label class="setting-toggle">
      <span>最小化到托盘</span>
      <input type="checkbox" checked />
    </label>
    <label class="setting-toggle">
      <span>自动检查更新</span>
      <input type="checkbox" disabled />
    </label>
  </aside>
</template>

<style scoped>
.runtime-panel {
  min-width: 0;
  min-height: 0;
  overflow: hidden;
  padding: 12px 13px 10px;
  border: 1px solid rgba(218, 226, 237, 0.95);
  border-radius: var(--radius-md, 8px);
  background: rgba(255, 255, 255, 0.96);
  box-shadow: var(--shadow-panel);
}

h2 {
  margin-bottom: 9px;
  color: #111827;
  font-size: 13px;
  line-height: 1.15;
  font-weight: 800;
}

.runtime-list {
  display: grid;
  gap: 7px;
  margin-bottom: 8px;
}

.runtime-list div {
  min-width: 0;
  display: grid;
  grid-template-columns: 74px minmax(0, 1fr);
  align-items: center;
  gap: 8px;
}

dt {
  color: #596579;
  font-size: 11px;
  font-weight: 700;
}

dd {
  min-width: 0;
  color: #111827;
  font-size: 11px;
  font-weight: 700;
  text-align: right;
  white-space: nowrap;
}

.update-row dd {
  text-align: right;
}

.check-button {
  height: 25px;
  display: inline-flex;
  align-items: center;
  gap: 5px;
  padding: 0 8px;
  border: 1px solid #d8e1ec;
  border-radius: 4px;
  background: #ffffff;
  color: #4b5563;
  font-size: 11px;
  font-weight: 700;
}

.check-button:disabled {
  cursor: wait;
  opacity: 0.64;
}

.spinning {
  animation: spin 0.8s linear infinite;
}

@keyframes spin {
  to {
    transform: rotate(360deg);
  }
}

.setting-toggle {
  height: 25px;
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 10px;
  color: #303847;
  font-size: 11px;
  font-weight: 700;
}

.setting-toggle input {
  width: 27px;
  height: 16px;
  accent-color: var(--color-primary);
}
</style>
