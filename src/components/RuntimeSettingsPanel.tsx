import { RefreshCw } from "lucide-react";

interface RuntimeSettingsPanelProps {
  uptime: number;
  version: string;
  autoStartEnabled: boolean;
  autoStartSaving: boolean;
  updateChecking: boolean;
  onToggleAutostart: (enabled: boolean) => void;
  onCheckUpdate: () => void;
}

function formattedUptime(uptime: number): string {
  const totalSeconds = Math.max(0, Math.floor(Number(uptime) || 0));
  const minutes = Math.floor(totalSeconds / 60);
  const hours = Math.floor(minutes / 60);
  const days = Math.floor(hours / 24);

  if (totalSeconds < 60) return `${totalSeconds} 秒`;
  if (minutes < 60) return `${minutes} 分钟`;
  if (hours < 24) return `${hours} 小时 ${minutes % 60} 分钟`;
  return `${days} 天 ${hours % 24} 小时`;
}

export default function RuntimeSettingsPanel({
  uptime,
  version,
  autoStartEnabled,
  autoStartSaving,
  updateChecking,
  onToggleAutostart,
  onCheckUpdate,
}: RuntimeSettingsPanelProps) {
  return (
    <aside className="runtime-panel">
      <h2>运行时与设置</h2>

      <dl className="runtime-list">
        <div>
          <dt>运行时长</dt>
          <dd>{formattedUptime(uptime)}</dd>
        </div>
        <div>
          <dt>版本信息</dt>
          <dd>{version}</dd>
        </div>
        <div className="update-row">
          <dt>检查更新</dt>
          <dd>
            <button className="check-button" type="button" disabled={updateChecking} onClick={onCheckUpdate}>
              <RefreshCw className={updateChecking ? "spinning" : undefined} size={13} strokeWidth={2.2} />
              检查更新
            </button>
          </dd>
        </div>
      </dl>

      <label className="setting-toggle">
        <span>开机自动启动</span>
        <input
          type="checkbox"
          checked={autoStartEnabled}
          disabled={autoStartSaving}
          onChange={(event) => onToggleAutostart(event.target.checked)}
        />
      </label>
      <label className="setting-toggle">
        <span>最小化到托盘</span>
        <input type="checkbox" checked readOnly />
      </label>
      <label className="setting-toggle">
        <span>自动检查更新</span>
        <input type="checkbox" disabled />
      </label>
    </aside>
  );
}
