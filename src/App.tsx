import { useCallback, useEffect, useMemo, useState } from "react";
import { getSystemSnapshot, isDesktopRuntime } from "./api";
import type { DiskInfo, SystemSnapshot } from "./types";
import "./styles.css";

type Tab = "overview" | "hardware" | "processes" | "report";

const navItems: Array<{ id: Tab; label: string; icon: string }> = [
  { id: "overview", label: "总览", icon: "⌂" },
  { id: "hardware", label: "硬件与存储", icon: "▣" },
  { id: "processes", label: "进程", icon: "≡" },
  { id: "report", label: "诊断报告", icon: "↗" },
];

function bytes(value: number): string {
  if (value >= 1024 ** 3) return `${(value / 1024 ** 3).toFixed(1)} GB`;
  if (value >= 1024 ** 2) return `${(value / 1024 ** 2).toFixed(0)} MB`;
  return `${Math.round(value / 1024)} KB`;
}

function duration(seconds: number): string {
  const days = Math.floor(seconds / 86_400);
  const hours = Math.floor((seconds % 86_400) / 3_600);
  const minutes = Math.floor((seconds % 3_600) / 60);
  return `${days} 天 ${hours} 小时 ${minutes} 分钟`;
}

function Ring({ value, label, color }: { value: number; label: string; color: string }) {
  const safe = Math.min(100, Math.max(0, value));
  return (
    <div className="ring-wrap">
      <div className="ring" style={{ background: `conic-gradient(${color} ${safe}%, #e9eef5 0)` }}>
        <div className="ring-center"><strong>{safe.toFixed(0)}%</strong><span>{label}</span></div>
      </div>
    </div>
  );
}

function MetricCard({ title, value, detail, tone = "blue" }: { title: string; value: string; detail: string; tone?: string }) {
  return <article className={`metric-card ${tone}`}><span>{title}</span><strong>{value}</strong><small>{detail}</small></article>;
}

function DiskRow({ disk }: { disk: DiskInfo }) {
  const used = disk.totalBytes - disk.availableBytes;
  const percent = disk.totalBytes ? (used / disk.totalBytes) * 100 : 0;
  return (
    <div className="disk-row">
      <div><strong>{disk.name || disk.mountPoint}</strong><span>{disk.mountPoint} · {disk.fileSystem || "未知文件系统"}</span></div>
      <div className="disk-usage"><div className="progress"><i style={{ width: `${percent}%` }} /></div><span>{bytes(used)} / {bytes(disk.totalBytes)}</span></div>
    </div>
  );
}

function App() {
  const [tab, setTab] = useState<Tab>("overview");
  const [snapshot, setSnapshot] = useState<SystemSnapshot | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    setLoading(true);
    try {
      setSnapshot(await getSystemSnapshot());
      setError(null);
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause));
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => { void refresh(); }, [refresh]);
  useEffect(() => {
    const timer = window.setInterval(() => { void refresh(); }, 5_000);
    return () => window.clearInterval(timer);
  }, [refresh]);

  const report = useMemo(() => snapshot ? JSON.stringify({
    schemaVersion: 1,
    generatedAt: new Date(snapshot.capturedAtUnixMs).toISOString(),
    privacy: "主机名已脱敏",
    system: { ...snapshot, hostName: "[已脱敏]", processes: snapshot.processes.map(({ pid: _pid, ...process }) => process) },
  }, null, 2) : "", [snapshot]);

  const downloadReport = () => {
    const blob = new Blob([report], { type: "application/json" });
    const url = URL.createObjectURL(blob);
    const anchor = document.createElement("a");
    anchor.href = url;
    anchor.download = `system-lens-report-${new Date().toISOString().slice(0, 10)}.json`;
    anchor.click();
    URL.revokeObjectURL(url);
  };

  if (!snapshot && loading) return <div className="splash"><div className="brand-mark">S</div><p>正在读取系统状态…</p></div>;
  if (!snapshot) return <div className="splash error"><p>无法读取系统状态</p><code>{error}</code><button onClick={refresh}>重试</button></div>;

  const memoryFree = snapshot.memory.totalBytes - snapshot.memory.usedBytes;
  return (
    <div className="app-shell">
      <aside className="sidebar">
        <div className="brand"><div className="brand-mark">S</div><div><strong>小鲁</strong><span>System Lens</span></div></div>
        <nav>{navItems.map(item => <button key={item.id} className={tab === item.id ? "active" : ""} onClick={() => setTab(item.id)}><b>{item.icon}</b>{item.label}</button>)}</nav>
        <div className="privacy-card"><span>本地优先</span><p>系统数据只在本机处理，不上传云端。</p></div>
        <div className="version">v0.1.0 · 只读模式</div>
      </aside>

      <main>
        <header><div><p className="eyebrow">{snapshot.hostName}</p><h1>{navItems.find(item => item.id === tab)?.label}</h1></div><div className="header-actions">{!isDesktopRuntime() && <span className="demo-badge">浏览器演示数据</span>}<button className="refresh" onClick={refresh} disabled={loading}>{loading ? "刷新中…" : "刷新"}</button></div></header>
        {error && <div className="error-banner">上次刷新失败：{error}</div>}

        {tab === "overview" && <>
          <section className="hero-card"><div><span className="status-dot" /> 系统运行正常<h2>{snapshot.osName} {snapshot.osVersion}</h2><p>{snapshot.cpu.brand} · {snapshot.memory.totalBytes ? bytes(snapshot.memory.totalBytes) : "内存未知"}</p><div className="hero-meta"><span>架构 {snapshot.architecture}</span><span>已运行 {duration(snapshot.uptimeSeconds)}</span><span>内核 {snapshot.kernelVersion}</span></div></div><div className="rings"><Ring value={snapshot.cpu.usagePercent} label="CPU" color="#3269f5" /><Ring value={snapshot.memory.usagePercent} label="内存" color="#25b8a6" /></div></section>
          <section className="metric-grid"><MetricCard title="CPU 使用率" value={`${snapshot.cpu.usagePercent.toFixed(1)}%`} detail={`${snapshot.cpu.logicalCores} 逻辑核心 · ${snapshot.cpu.frequencyMhz} MHz`} /><MetricCard title="可用内存" value={bytes(memoryFree)} detail={`共 ${bytes(snapshot.memory.totalBytes)}`} tone="green" /><MetricCard title="磁盘数量" value={String(snapshot.disks.length)} detail="本地与可移动存储" tone="purple" /><MetricCard title="活跃进程" value={String(snapshot.processes.length)} detail="展示资源占用最高项" tone="orange" /></section>
          <section className="panel"><div className="panel-head"><div><span className="eyebrow">STORAGE</span><h3>存储空间</h3></div><button onClick={() => setTab("hardware")}>查看详情</button></div>{snapshot.disks.slice(0, 3).map(disk => <DiskRow key={`${disk.name}-${disk.mountPoint}`} disk={disk} />)}</section>
        </>}

        {tab === "hardware" && <section className="content-grid"><article className="panel detail-card"><span className="eyebrow">PROCESSOR</span><h3>{snapshot.cpu.brand}</h3><dl><div><dt>逻辑核心</dt><dd>{snapshot.cpu.logicalCores}</dd></div><div><dt>当前频率</dt><dd>{snapshot.cpu.frequencyMhz} MHz</dd></div><div><dt>当前负载</dt><dd>{snapshot.cpu.usagePercent.toFixed(1)}%</dd></div></dl></article><article className="panel detail-card"><span className="eyebrow">MEMORY</span><h3>{bytes(snapshot.memory.totalBytes)} 内存</h3><dl><div><dt>已使用</dt><dd>{bytes(snapshot.memory.usedBytes)}</dd></div><div><dt>可用</dt><dd>{bytes(memoryFree)}</dd></div><div><dt>使用率</dt><dd>{snapshot.memory.usagePercent.toFixed(1)}%</dd></div></dl></article><article className="panel full"><div className="panel-head"><div><span className="eyebrow">DISKS</span><h3>磁盘与分区</h3></div><span>{snapshot.disks.length} 个</span></div>{snapshot.disks.map(disk => <DiskRow key={`${disk.name}-${disk.mountPoint}`} disk={disk} />)}</article></section>}

        {tab === "processes" && <section className="panel"><div className="panel-head"><div><span className="eyebrow">PROCESSES</span><h3>资源占用最高的进程</h3></div><span>每 5 秒更新</span></div><div className="process-table"><div className="process-row heading"><span>名称</span><span>PID</span><span>CPU</span><span>内存</span></div>{snapshot.processes.map(process => <div className="process-row" key={process.pid}><strong>{process.name}</strong><span>{process.pid}</span><span>{process.cpuPercent.toFixed(1)}%</span><span>{bytes(process.memoryBytes)}</span></div>)}</div></section>}

        {tab === "report" && <section className="report-layout"><article className="panel"><span className="eyebrow">PRIVACY-FIRST REPORT</span><h3>诊断报告</h3><p className="muted">报告包含系统、CPU、内存、磁盘和进程摘要。主机名与 PID 会在导出时自动移除。</p><ul className="check-list"><li>不包含文件内容</li><li>不包含浏览历史</li><li>不上传网络</li><li>导出前可预览</li></ul><button className="primary" onClick={downloadReport}>导出 JSON 报告</button></article><pre className="report-preview">{report}</pre></section>}
      </main>
    </div>
  );
}

export default App;
