<script setup lang="ts">
import { computed } from "vue";
import type { Component } from "vue";
import { Globe, Monitor, ShieldCheck, Workflow } from "@lucide/vue";

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
};

const iconComponent = computed(() => iconMap[props.icon]);
</script>

<template>
  <article
    class="status-card"
    :class="[`status-${status}`, { 'has-control': $slots.control }]"
  >
    <div class="icon-disc" :class="`icon-${icon}`" aria-hidden="true">
      <component
        v-if="iconComponent"
        :is="iconComponent"
        :size="22"
        :stroke-width="2.4"
      />
    </div>
    <div class="card-copy">
      <h3>{{ title }}</h3>
      <p class="card-value">{{ value }}</p>
      <p class="card-subtitle">{{ subtitle }}</p>
      <div v-if="$slots.control" class="card-control">
        <slot name="control"></slot>
      </div>
    </div>
  </article>
</template>

<style scoped>
.status-card {
  min-width: 0;
  height: 118px;
  display: grid;
  grid-template-columns: 42px minmax(0, 1fr);
  column-gap: 14px;
  align-items: start;
  padding: 19px 20px 15px;
  border: 1px solid rgba(217, 225, 237, 0.92);
  border-radius: var(--radius-md, 8px);
  background: rgba(255, 255, 255, 0.92);
  box-shadow: var(--shadow-card);
}

.icon-disc {
  width: 38px;
  height: 38px;
  display: grid;
  place-items: center;
  position: relative;
  border-radius: 50%;
  background: #e8f1ff;
  color: var(--color-primary);
}

.icon-globe {
  background: #eaf2ff;
  box-shadow: inset 0 0 0 1px #d6e5ff;
}

.icon-ipv6 {
  background: linear-gradient(135deg, #7c3aed, #2563eb);
  color: #ffffff;
  box-shadow: 0 4px 10px rgba(76, 78, 255, 0.28);
}

.icon-ipv6::before {
  content: "IP6";
  color: #ffffff;
  font-size: 10px;
  font-weight: 800;
  letter-spacing: 0;
}

.icon-shield {
  background: #e8f8ee;
  color: #16a34a;
}

.icon-rules {
  background: #e8f1ff;
}

.icon-devices {
  background: #e3fbfb;
  color: #14b8a6;
}

.icon-disc svg {
  display: block;
}

.card-copy {
  min-width: 0;
}

h3 {
  margin-top: 0;
  font-size: 15px;
  font-weight: 700;
  color: #202532;
}

.card-value {
  margin-top: 6px;
  font-size: 21px;
  line-height: 1.2;
  font-weight: 800;
  color: #05070b;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  font-variant-numeric: tabular-nums;
}

.status-success .card-value {
  color: #16a34a;
}

.status-warning .card-value {
  color: #d97706;
}

.status-error .card-value {
  color: #dc2626;
}

.card-subtitle {
  margin-top: 10px;
  color: #687386;
  font-size: 12px;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.has-control {
  padding-top: 14px;
  padding-bottom: 12px;
}

.has-control .card-value {
  margin-top: 4px;
  font-size: 19px;
}

.has-control .card-subtitle {
  margin-top: 5px;
  font-size: 11px;
}

.card-control {
  margin-top: 5px;
  min-width: 0;
}
</style>
