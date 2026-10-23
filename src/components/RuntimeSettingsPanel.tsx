import { RefreshCw } from "lucide-react";
import type { ThemeMode } from "../types";
import { formatUptime } from "../utils/presentation";

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
  theme: ThemeMode;
  onThemeChange: (theme: ThemeMode) => void;
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
  theme,
  onThemeChange,
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
    <section className="panel runtime-panel">
      <header className="panel-header">
        <h2>外观与运行偏好</h2>
      </header>

      <label className="setting-toggle">
        <span>
          <strong>主题外观</strong>
          <small>主窗口与菜单栏小窗保持一致</small>
        </span>
        <select
          aria-label="主题外观"
          value={theme}
          onChange={(event) => onThemeChange(event.target.value as ThemeMode)}
        >
          <option value="system">跟随系统</option>
          <option value="dark">石墨深色</option>
          <option value="light">柔和浅色</option>
        </select>
      </label>

      <dl className="runtime-list">
        <div>
          <dt>运行时长</dt>
          <dd>{formatUptime(uptime)}</dd>
        </div>
        <div>
          <dt>版本信息</dt>
          <dd>{version}</dd>
        </div>
        <div className="update-row">
          <dt>检查更新</dt>
          <dd>
            <button
              className="check-button"
              type="button"
              disabled={updateBusy}
              onClick={onUpdateAction}
            >
              <RefreshCw
                className={updateBusy ? "spinning" : undefined}
                size={13}
                strokeWidth={2.2}
              />
              {updateButtonLabel}
            </button>
          </dd>
        </div>
      </dl>

      <label className="setting-toggle">
        <span>
          <strong>开机自动启动</strong>
          <small>登录系统后自动启动 HomeNet</small>
        </span>
        <input
          type="checkbox"
          checked={autoStartEnabled}
          disabled={autoStartSaving}
          onChange={(event) => onToggleAutostart(event.target.checked)}
        />
      </label>
      <label className="setting-toggle">
        <span>
          <strong>自动检查更新</strong>
          <small>每 6 小时检查新版本，安装前由你确认</small>
        </span>
        <input
          type="checkbox"
          checked={autoUpdateCheckEnabled}
          disabled={autoUpdateCheckSaving}
          onChange={(event) => onToggleAutoUpdateCheck(event.target.checked)}
        />
      </label>
    </section>
  );
}
