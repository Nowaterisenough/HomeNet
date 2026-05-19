<script setup lang="ts">
import { computed } from "vue";
import type { Component } from "vue";
import { Globe, Monitor, Network, Server, ShieldCheck, Workflow } from "@lucide/vue";

const props = defineProps<{
  title: string;
  value: string;
  subtitle: string;
  status: "normal" | "warning" | "error" | "success";
  icon: string;
}>();

const iconMap: Record<string, Component> = {
  globe: Globe,
  shield: ShieldCheck,
  rules: Workflow,
  devices: Monitor,
  proxy: Server,
};

const iconComponent = computed(() => iconMap[props.icon] ?? Network);
</script>

<template>
  <article class="status-card" :class="[`status-${status}`, `card-${icon}`]">
    <div class="icon-disc" :class="`icon-${icon}`" aria-hidden="true">
      <span v-if="icon === 'ipv6'" class="ipv6-mark">非</span>
      <component
        v-else
        :is="iconComponent"
        :size="28"
        :stroke-width="2.45"
      />
    </div>

    <div class="card-copy">
      <h3>{{ title }}</h3>
      <p class="card-value">{{ value }}</p>
      <p class="card-subtitle">{{ subtitle }}</p>
    </div>
  </article>
</template>

<style scoped>
.status-card {
  min-width: 0;
  height: 106px;
  display: grid;
  grid-template-columns: 42px minmax(0, 1fr);
  column-gap: 13px;
  align-items: start;
  padding: 16px 22px 13px;
  border: 1px solid rgba(218, 226, 237, 0.95);
  border-radius: var(--radius-md, 8px);
  background: rgba(255, 255, 255, 0.96);
  box-shadow: var(--shadow-panel);
}

.icon-disc {
  width: 38px;
  height: 38px;
  display: grid;
  place-items: center;
  border-radius: 50%;
  background: #e9f2ff;
  color: var(--color-primary);
}

.icon-globe {
  color: #0d69ff;
  background: #eaf2ff;
}

.icon-ipv6 {
  color: #ffffff;
  background: radial-gradient(circle at 35% 28%, #8f7bff 0 24%, #5a47ff 56%, #3926d8 100%);
  box-shadow: inset 0 0 0 2px rgba(255, 255, 255, 0.35);
}

.ipv6-mark {
  color: #ffffff;
  font-size: 22px;
  line-height: 1;
  font-weight: 900;
}

.icon-shield {
  color: #ffffff;
  background: #10a861;
}

.icon-rules {
  color: #0d69ff;
  background: #e8f1ff;
}

.icon-proxy,
.icon-devices {
  color: #0aa6c7;
  background: #e5f9fd;
}

.card-copy {
  min-width: 0;
}

h3 {
  color: #171c28;
  font-size: 13px;
  line-height: 1.18;
  font-weight: 800;
  white-space: nowrap;
}

.card-value {
  margin-top: 7px;
  color: #04070d;
  font-size: 21px;
  line-height: 1.1;
  font-weight: 800;
  letter-spacing: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-variant-numeric: tabular-nums;
}

.card-ipv6 .card-value {
  font-size: 16px;
}

.status-success .card-value {
  color: var(--color-success);
}

.status-warning .card-value {
  color: var(--color-warning);
}

.status-error .card-value {
  color: var(--color-error);
}

.card-subtitle {
  margin-top: 8px;
  color: #596579;
  font-size: 11px;
  line-height: 1.2;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
</style>
