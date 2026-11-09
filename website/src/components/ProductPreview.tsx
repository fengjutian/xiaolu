import { useState } from "react";
import { Activity, HardDrive, LayoutDashboard, Thermometer } from "lucide-react";

const views = {
  overview: {
    label: "系统概览",
    icon: LayoutDashboard,
    score: "92",
    detail: "状态良好",
    metrics: [
      ["CPU", "27%", "#7c8cff"],
      ["内存", "43%", "#62d9ad"],
      ["磁盘", "58%", "#f2b45f"],
    ],
  },
  sensors: {
    label: "温度监控",
    icon: Thermometer,
    score: "54°",
    detail: "温度正常",
    metrics: [
      ["显卡", "54°C", "#7c8cff"],
      ["硬盘", "39°C", "#62d9ad"],
      ["风扇", "32%", "#f2b45f"],
    ],
  },
  storage: {
    label: "存储健康",
    icon: HardDrive,
    score: "96%",
    detail: "健康度",
    metrics: [
      ["可用空间", "412 GB", "#7c8cff"],
      ["累计写入", "8.2 TB", "#62d9ad"],
      ["通电时间", "1260 h", "#f2b45f"],
    ],
  },
} as const;

type ViewKey = keyof typeof views;

export default function ProductPreview() {
  const [active, setActive] = useState<ViewKey>("overview");
  const view = views[active];
  const ActiveIcon = view.icon;

  return (
    <div className="preview-window" aria-label="小鲁产品界面演示">
      <div className="preview-titlebar">
        <img className="preview-mark" src="/favicon.png" alt="" width="26" height="26" />
        <span>小鲁 · 系统透镜</span>
        <span className="preview-status"><Activity size={13} /> 实时监测中</span>
      </div>
      <div className="preview-body">
        <nav className="preview-nav" aria-label="演示视图">
          {(Object.keys(views) as ViewKey[]).map((key) => {
            const Icon = views[key].icon;
            return (
            <button
              className={active === key ? "active" : ""}
              key={key}
              onClick={() => setActive(key)}
              type="button"
            >
              <Icon size={17} strokeWidth={1.8} aria-hidden="true" />
              {views[key].label}
            </button>
          )})}
        </nav>
        <section className="preview-content" aria-live="polite">
          <div className="preview-heading">
            <div><small>设备状态</small><h3><ActiveIcon size={22} aria-hidden="true" />{view.label}</h3></div>
            <span>刚刚更新</span>
          </div>
          <div className="score-card">
            <div className="score-ring"><strong>{view.score}</strong><span>{view.detail}</span></div>
            <div className="score-copy"><span>系统运行平稳</span><p>核心硬件状态清晰可见，异常情况及时提醒。</p></div>
          </div>
          <div className="metric-grid">
            {view.metrics.map(([name, value, color], index) => (
              <article key={name}>
                <span>{name}</span><strong>{value}</strong>
                <div><i style={{ width: `${48 + index * 16}%`, background: color }} /></div>
              </article>
            ))}
          </div>
          <p className="demo-note">浏览器演示 · 数据为模拟示例</p>
        </section>
      </div>
    </div>
  );
}
