<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from "vue";
import { api, UsageReport } from "../api";

const WINDOWS = ["1h", "1d", "7d", "30d"] as const;
type Win = (typeof WINDOWS)[number];

const win = ref<Win>("1d");
const report = ref<UsageReport | null>(null);
const error = ref("");
const loading = ref(false);

const palette = [
  "#e23744",
  "#2e7d32",
  "#1565c0",
  "#f9a825",
  "#6a1b9a",
  "#00838f",
  "#8d6e63",
  "#c2185b",
];

// stable model -> color mapping based on model_totals order
const modelColors = computed<Record<string, string>>(() => {
  const map: Record<string, string> = {};
  const r = report.value;
  if (!r) return map;
  Object.keys(r.model_totals).forEach((m, i) => {
    map[m] = palette[i % palette.length];
  });
  return map;
});

const models = computed(() => (report.value ? Object.keys(report.value.model_totals) : []));

const maxBucket = computed(() => {
  const r = report.value;
  if (!r) return 0;
  return r.buckets.reduce((m, b) => Math.max(m, b.total), 0);
});

const CHART_H = 160;
const AXIS_H = 24;
const BAR_FILL = 0.62; // bar width as a fraction of its slot

// The chart fills the full content width of its (padded) container; we measure
// that width with a ResizeObserver and lay bars out evenly across it.
const chartWrap = ref<HTMLElement | null>(null);
const containerW = ref(0);
let ro: ResizeObserver | null = null;

const bucketCount = computed(() => report.value?.buckets.length ?? 0);
const chartWidth = computed(() => Math.max(containerW.value, 1));
const slotW = computed(() =>
  bucketCount.value > 0 ? chartWidth.value / bucketCount.value : 0,
);
const barW = computed(() => Math.max(slotW.value * BAR_FILL, 1));

function measure() {
  const el = chartWrap.value;
  if (!el) return;
  const cs = getComputedStyle(el);
  const pad = parseFloat(cs.paddingLeft || "0") + parseFloat(cs.paddingRight || "0");
  containerW.value = Math.max(0, el.clientWidth - pad);
}

interface Seg {
  model: string;
  y: number;
  h: number;
  color: string;
  value: number;
}

// for each bucket, stacked segments (bottom-up) scaled to CHART_H
function segmentsFor(bucketIndex: number): Seg[] {
  const r = report.value;
  if (!r || maxBucket.value === 0) return [];
  const bucket = r.buckets[bucketIndex];
  const segs: Seg[] = [];
  let acc = 0;
  for (const m of models.value) {
    const v = bucket.models[m]?.total ?? 0;
    if (v <= 0) continue;
    const h = (v / maxBucket.value) * CHART_H;
    const y = CHART_H - acc - h;
    segs.push({ model: m, y, h, color: modelColors.value[m] ?? "#888", value: v });
    acc += h;
  }
  return segs;
}

function barX(i: number): number {
  return i * slotW.value + (slotW.value - barW.value) / 2;
}

function labelFor(b: { t: number }): string {
  const d = new Date(b.t);
  if (win.value === "1h" || win.value === "1d") {
    return `${String(d.getHours()).padStart(2, "0")}:${String(d.getMinutes()).padStart(2, "0")}`;
  }
  return `${d.getMonth() + 1}/${d.getDate()}`;
}

// show ~6 axis labels regardless of bucket count
const labelEvery = computed(() => {
  const n = report.value?.buckets.length ?? 0;
  return Math.max(1, Math.ceil(n / 6));
});

async function load() {
  loading.value = true;
  error.value = "";
  try {
    report.value = await api.usage(win.value);
  } catch (e: any) {
    error.value = e?.message ?? String(e);
  } finally {
    loading.value = false;
  }
}

function setWindow(w: Win) {
  win.value = w;
  load();
}

function fmtNum(n: number): string {
  if (n >= 1_000_000) return (n / 1_000_000).toFixed(1) + "M";
  if (n >= 1000) return (n / 1000).toFixed(1) + "k";
  return String(n);
}

onMounted(() => {
  load();
  measure();
  ro = new ResizeObserver(() => measure());
  if (chartWrap.value) ro.observe(chartWrap.value);
});

onBeforeUnmount(() => {
  ro?.disconnect();
  ro = null;
});
</script>

<template>
  <p v-if="error" class="notice bad">{{ error }}</p>

  <section class="panel">
    <div class="panel-heading">
      <p class="section-number">01</p>
      <div>
        <h2>Token 用量</h2>
        <p>按模型统计的 token 消耗。柱状图为各时间桶的总量，按模型堆叠。</p>
      </div>
    </div>

    <div class="seg-tabs">
      <button
        v-for="w in WINDOWS"
        :key="w"
        :class="['seg-tab', { active: win === w }]"
        @click="setWindow(w)"
      >
        {{ w }}
      </button>
      <span class="grand">合计 {{ fmtNum(report?.grand_total ?? 0) }} tokens</span>
    </div>

    <div class="chart-wrap" ref="chartWrap">
      <svg
        v-if="report && maxBucket > 0 && containerW > 0"
        class="bars"
        :viewBox="`0 0 ${chartWidth} ${CHART_H + AXIS_H}`"
        preserveAspectRatio="none"
        role="img"
        aria-label="token 用量柱状图"
      >
        <g v-for="(b, i) in report.buckets" :key="i">
          <rect
            v-for="(s, si) in segmentsFor(i)"
            :key="si"
            :x="barX(i)"
            :y="s.y"
            :width="barW"
            :height="Math.max(s.h, s.value > 0 ? 1 : 0)"
            :fill="s.color"
          >
            <title>{{ labelFor(b) }} · {{ s.model }}: {{ s.value }}</title>
          </rect>
          <text
            v-if="i % labelEvery === 0"
            :x="barX(i) + barW / 2"
            :y="CHART_H + 16"
            text-anchor="middle"
            class="axis-label"
          >
            {{ labelFor(b) }}
          </text>
        </g>
      </svg>
      <p v-else-if="report" class="notice">该时间段内还没有用量记录。</p>
      <p v-else class="notice">加载中…</p>
    </div>

    <div v-if="models.length" class="legend">
      <span v-for="m in models" :key="m" class="legend-item">
        <i :style="{ background: modelColors[m] }"></i>
        {{ m }} · {{ fmtNum(report?.model_totals[m] ?? 0) }}
      </span>
    </div>
  </section>
</template>

<style scoped>
.seg-tabs {
  display: flex;
  align-items: center;
  padding: 0 32px 32px 120px;
}
.seg-tab {
  padding: 0.8rem 1.5rem;
  border-top: 1px solid var(--ink);
  border-bottom: 1px solid var(--ink);
  border-left: 0;
  border-right: 0;
  background: transparent;
  color: var(--ink);
  cursor: pointer;
  font: inherit;
  font-weight: 700;
}
.seg-tab:first-of-type {
  border-left: 1px solid var(--ink);
}
.seg-tab:last-of-type {
  border-right: 1px solid var(--ink);
}
.seg-tab.active {
  background: var(--ink);
  color: var(--bg);
}
.seg-tabs .grand {
  margin-left: auto;
  opacity: 0.7;
  font-size: 0.9em;
}
.chart-wrap {
  width: 100%;
  padding: 0 32px 32px 120px;
}
.bars {
  display: block;
  width: 100%;
  height: 184px;
}
.axis-label {
  font-size: 9px;
  fill: currentColor;
  opacity: 0.55;
}
.legend {
  display: flex;
  flex-wrap: wrap;
  gap: 0.4rem 1.1rem;
  padding: 0 32px 32px 120px;
  font-size: 0.85em;
}
.legend-item {
  display: inline-flex;
  align-items: center;
  gap: 0.4rem;
}
.legend-item i {
  width: 11px;
  height: 11px;
  border-radius: 2px;
  display: inline-block;
}
</style>
