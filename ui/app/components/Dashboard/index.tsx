"use client";

import { useEffect, useRef, useState } from "react";

// One metric line from the aggregated dashboard endpoint.
type Sample = { name: string; labels: Record<string, string>; value: number };
type Dash = { samples: Sample[]; sources: Record<string, string> };

const POLL_MS = 5000;
const HISTORY = 60; // keep the last 60 samples for the charts

// Sum the value of every sample with `name` that matches the label filter.
function sum(s: Sample[], name: string, f: Record<string, string> = {}): number {
  return s
    .filter((x) => x.name === name && Object.entries(f).every(([k, v]) => x.labels[k] === v))
    .reduce((a, x) => a + x.value, 0);
}

const mb = (bytes: number) => bytes / (1024 * 1024);

// A small inline SVG line chart.
function Sparkline({ data, color, unit }: { data: number[]; color: string; unit?: string }) {
  const w = 240;
  const h = 48;
  if (data.length < 2) {
    return <div className="text-xs text-gray-400 h-12 flex items-center">collecting…</div>;
  }
  const max = Math.max(...data, 0.0001);
  const min = Math.min(...data, 0);
  const span = max - min || 1;
  const pts = data
    .map((v, i) => `${(i / (data.length - 1)) * w},${h - ((v - min) / span) * h}`)
    .join(" ");
  return (
    <div>
      <svg width={w} height={h} className="w-full" viewBox={`0 0 ${w} ${h}`} preserveAspectRatio="none">
        <polyline points={pts} fill="none" stroke={color} strokeWidth="2" />
      </svg>
      <div className="text-xs text-gray-400 mt-1">
        now {data[data.length - 1].toFixed(2)}
        {unit ? ` ${unit}` : ""} · max {max.toFixed(2)}
      </div>
    </div>
  );
}

function Stat({ label, value, sub }: { label: string; value: string; sub?: string }) {
  return (
    <div className="bg-white rounded-lg border border-gray-200 p-4">
      <div className="text-xs uppercase tracking-wide text-gray-500">{label}</div>
      <div className="text-2xl font-semibold text-gray-900 mt-1">{value}</div>
      {sub && <div className="text-xs text-gray-400 mt-1">{sub}</div>}
    </div>
  );
}

export default function Dashboard() {
  const [dash, setDash] = useState<Dash | null>(null);
  const [error, setError] = useState<string | null>(null);
  // Rolling history for the charts.
  const [hist, setHist] = useState<{ latency: number[]; reqRate: number[]; cpu: number[]; findings: number[] }>({
    latency: [],
    reqRate: [],
    cpu: [],
    findings: [],
  });
  // Previous counter snapshot, to turn counters into rates.
  const prev = useRef<{ t: number; reqCount: number; findings: number } | null>(null);

  useEffect(() => {
    let alive = true;
    const tick = async () => {
      try {
        const res = await fetch(`${process.env.NEXT_PUBLIC_API_URL}/dashboard`);
        if (!res.ok) throw new Error(`HTTP ${res.status}`);
        const data: Dash = await res.json();
        if (!alive) return;
        setDash(data);
        setError(null);

        const s = data.samples;
        const t = Date.now();
        const reqCount = sum(s, "striem_request_duration_seconds_count", { job: "api" });
        const reqSum = sum(s, "striem_request_duration_seconds_sum", { job: "api" });
        const findings = sum(s, "striem_events_total", { job: "detection", kind: "findings" });
        const avgLatencyMs = reqCount > 0 ? (reqSum / reqCount) * 1000 : 0;
        const cpu = sum(s, "striem_resource", { job: "api", resource: "process_cpu_percent" });

        let reqRate = 0;
        let findingsRate = 0;
        if (prev.current) {
          const dt = (t - prev.current.t) / 1000 || 1;
          reqRate = Math.max(0, (reqCount - prev.current.reqCount) / dt);
          findingsRate = Math.max(0, (findings - prev.current.findings) / dt);
        }
        prev.current = { t, reqCount, findings };

        setHist((h) => {
          const push = (arr: number[], v: number) => [...arr, v].slice(-HISTORY);
          return {
            latency: push(h.latency, avgLatencyMs),
            reqRate: push(h.reqRate, reqRate),
            cpu: push(h.cpu, cpu),
            findings: push(h.findings, findingsRate),
          };
        });
      } catch (e) {
        if (alive) setError(e instanceof Error ? e.message : "failed to load metrics");
      }
    };
    tick();
    const id = setInterval(tick, POLL_MS);
    return () => {
      alive = false;
      clearInterval(id);
    };
  }, []);

  const s = dash?.samples ?? [];

  const reqCount = sum(s, "striem_request_duration_seconds_count", { job: "api" });
  const reqSum = sum(s, "striem_request_duration_seconds_sum", { job: "api" });
  const avgLatencyMs = reqCount > 0 ? (reqSum / reqCount) * 1000 : 0;

  const resource = (job: string, name: string) => sum(s, "striem_resource", { job, resource: name });
  const events = (kind: string) => sum(s, "striem_events_total", { job: "detection", kind });

  // Vector throughput: received/sent events across all components.
  const vectorRecv = sum(s, "vector_component_received_events_total", { job: "vector" });
  const vectorSent = sum(s, "vector_component_sent_events_total", { job: "vector" });

  return (
    <div className="h-full overflow-y-auto bg-gray-50">
      <div className="border-b border-gray-200 bg-white px-6 py-4">
        <h1 className="text-xl font-semibold text-gray-900">Dashboard</h1>
        <p className="text-sm text-gray-500">Latency, resources, and pipeline throughput. Updates every 5s.</p>
      </div>

      <div className="p-6 space-y-6">
        {error && <div className="error-container"><span className="error-text">Metrics error: {error}</span></div>}

        {/* Source availability */}
        <div className="flex flex-wrap gap-2">
          {dash &&
            Object.entries(dash.sources).map(([name, status]) => (
              <span
                key={name}
                className={`text-xs px-2 py-1 rounded-full border ${
                  status === "ok"
                    ? "bg-green-50 border-green-200 text-green-700"
                    : "bg-gray-100 border-gray-200 text-gray-500"
                }`}
              >
                {name}: {status}
              </span>
            ))}
        </div>

        {/* Latency + throughput */}
        <div className="grid grid-cols-2 md:grid-cols-4 gap-4">
          <Stat label="API latency (avg)" value={`${avgLatencyMs.toFixed(1)} ms`} sub={`${reqCount.toFixed(0)} requests`} />
          <Stat label="Events ingested" value={events("ingested").toLocaleString()} sub="detection engine" />
          <Stat label="Findings" value={events("findings").toLocaleString()} sub="detections emitted" />
          <Stat
            label="Vector events"
            value={vectorRecv.toLocaleString()}
            sub={`${vectorSent.toLocaleString()} sent`}
          />
        </div>

        {/* Resources */}
        <div className="grid grid-cols-2 md:grid-cols-4 gap-4">
          <Stat label="API CPU" value={`${resource("api", "process_cpu_percent").toFixed(1)} %`} />
          <Stat label="API memory" value={`${mb(resource("api", "process_memory_bytes")).toFixed(0)} MB`} />
          <Stat label="Detection CPU" value={`${resource("detection", "process_cpu_percent").toFixed(1)} %`} />
          <Stat
            label="System memory"
            value={`${mb(resource("api", "system_memory_used_bytes")).toFixed(0)} MB`}
            sub={`of ${mb(resource("api", "system_memory_total_bytes")).toFixed(0)} MB`}
          />
        </div>

        {/* Charts */}
        <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
          <div className="bg-white rounded-lg border border-gray-200 p-4">
            <div className="text-sm font-medium text-gray-700 mb-2">API latency (ms)</div>
            <Sparkline data={hist.latency} color="#2563eb" unit="ms" />
          </div>
          <div className="bg-white rounded-lg border border-gray-200 p-4">
            <div className="text-sm font-medium text-gray-700 mb-2">Request rate (req/s)</div>
            <Sparkline data={hist.reqRate} color="#16a34a" unit="/s" />
          </div>
          <div className="bg-white rounded-lg border border-gray-200 p-4">
            <div className="text-sm font-medium text-gray-700 mb-2">API CPU (%)</div>
            <Sparkline data={hist.cpu} color="#d97706" unit="%" />
          </div>
          <div className="bg-white rounded-lg border border-gray-200 p-4">
            <div className="text-sm font-medium text-gray-700 mb-2">Findings rate (/s)</div>
            <Sparkline data={hist.findings} color="#dc2626" unit="/s" />
          </div>
        </div>
      </div>
    </div>
  );
}
