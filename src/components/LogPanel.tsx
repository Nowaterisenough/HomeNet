import { useEffect, useMemo, useState } from "react";
import { RefreshCw, Trash2 } from "lucide-react";
import { invokeCommand } from "../lib/tauri";
import type { LogEntry } from "../types";

function levelClass(level: string): string {
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
}

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

export default function LogPanel() {
  const [logs, setLogs] = useState<LogEntry[]>([]);
  const [levelFilter, setLevelFilter] = useState("全部级别");
  const [refreshing, setRefreshing] = useState(false);

  const filteredLogs = useMemo(() => {
    if (levelFilter === "全部级别") return logs;
    return logs.filter((log) => log.level.toUpperCase() === levelFilter);
  }, [levelFilter, logs]);

  async function loadLogs() {
    setRefreshing(true);
    try {
      const data = await invokeCommand<LogEntry[]>("get_recent_logs");
      setLogs(data.slice().reverse());
    } catch {
      setLogs([]);
    } finally {
      setRefreshing(false);
    }
  }

  async function clearLogs() {
    try {
      await invokeCommand("clear_logs");
    } finally {
      setLogs([]);
    }
  }

  useEffect(() => {
    loadLogs();
    window.addEventListener("homenet:logs-refresh", loadLogs);
    const timer = window.setInterval(loadLogs, 5000);
    return () => {
      window.clearInterval(timer);
      window.removeEventListener("homenet:logs-refresh", loadLogs);
    };
  }, []);

  return (
    <section className="panel log-panel">
      <header className="panel-header">
        <h2>最近日志</h2>
        <div className="toolbar">
          <button className="btn btn-secondary" type="button" onClick={clearLogs}>
            <Trash2 size={13} strokeWidth={2.1} />
            清空
          </button>
          <select className="level-select" value={levelFilter} onChange={(event) => setLevelFilter(event.target.value)}>
            <option>全部级别</option>
            <option>INFO</option>
            <option>WARN</option>
            <option>ERROR</option>
          </select>
          <button className="btn btn-secondary" type="button" disabled={refreshing} onClick={loadLogs}>
            <RefreshCw className={refreshing ? "spinning" : undefined} size={13} strokeWidth={2.1} />
            刷新
          </button>
        </div>
      </header>

      <div className="log-table" role="table" aria-label="最近日志">
        <div className="log-head" role="row">
          <span>时间</span>
          <span>级别</span>
          <span>模块</span>
          <span>消息</span>
        </div>
        {filteredLogs.length === 0 ? <div className="empty-state">暂无日志记录</div> : null}
        {filteredLogs.map((log) => (
          <div key={log.id} className="log-row" role="row">
            <time className="log-time">{formatTime(log.time)}</time>
            <span className={`log-level ${levelClass(log.level)}`}>{log.level.toUpperCase()}</span>
            <span className="log-module">{log.module}</span>
            <span className="log-message">{log.message}</span>
          </div>
        ))}
      </div>
    </section>
  );
}
