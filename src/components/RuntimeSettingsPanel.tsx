import { RefreshCw } from "lucide-react";

interface RuntimeSettingsPanelProps {
  uptime: number;
  version: string;
  autoStartEnabled: boolean;
  autoStartSaving: boolean;
  autoUpdateCheckEnabled: boolean;
  autoUpdateCheckSaving: boolean;
  updateChecking: boolean;
  updateInstalling: boolean;
  availableUpdateVersion: string;
  onToggleAutostart: (enabled: boolean) => void;
  onToggleAutoUpdateCheck: (enabled: boolean) => void;
  onUpdateAction: () => void;
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
  autoUpdateCheckEnabled,
  autoUpdateCheckSaving,
  updateChecking,
  updateInstalling,
  availableUpdateVersion,
  onToggleAutostart,
  onToggleAutoUpdateCheck,
  onUpdateAction,
}: RuntimeSettingsPanelProps) {
  const updateBusy = updateChecking || updateInstalling;
  const updateButtonLabel = updateInstalling
    ? "正在安装"
    : updateChecking
      ? "正在检查"
      : availableUpdateVersion
        ? "安装更新"
        : "检查更新";

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
            <button className="check-button" type="button" disabled={updateBusy} onClick={onUpdateAction}>
              <RefreshCw className={updateBusy ? "spinning" : undefined} size={13} strokeWidth={2.2} />
              {updateButtonLabel}
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
        <input
          type="checkbox"
          checked={autoUpdateCheckEnabled}
          disabled={autoUpdateCheckSaving}
          onChange={(event) => onToggleAutoUpdateCheck(event.target.checked)}
        />
      </label>
    </aside>
  );
}
