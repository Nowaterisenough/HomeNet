import {
  useEffect,
  useId,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import { createPortal } from "react-dom";
import { Check, ChevronDown, Monitor, RefreshCw, Search } from "lucide-react";
import type { DeviceDdnsConfig, LanDevice } from "../types";
import { deviceRowId } from "../utils/devices";
import { devicePickerOptions } from "../utils/devicePicker";

interface Props {
  devices: LanDevice[];
  configs: DeviceDdnsConfig[];
  value: LanDevice | null;
  loading: boolean;
  error: string;
  disabled: boolean;
  onRefresh: () => Promise<void>;
  onChange: (device: LanDevice) => void;
}

export default function DevicePicker({
  devices,
  configs,
  value,
  loading,
  error,
  disabled,
  onRefresh,
  onChange,
}: Props) {
  const id = useId();
  const trigger = useRef<HTMLButtonElement>(null);
  const popup = useRef<HTMLDivElement>(null);
  const search = useRef<HTMLInputElement>(null);
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState("");
  const [activeId, setActiveId] = useState("");
  const [position, setPosition] = useState({
    top: 0,
    left: 0,
    width: 300,
    height: 340,
  });
  const options = useMemo(
    () => devicePickerOptions(devices, configs, query),
    [devices, configs, query],
  );
  const all = useMemo(
    () => devicePickerOptions(devices, configs),
    [devices, configs],
  );
  const selectedId = value ? deviceRowId(value) : "";
  const selected = all.find((option) => option.id === selectedId);
  const activeIndex = Math.max(
    0,
    options.findIndex((option) => option.id === activeId),
  );
  // Keeping the portal inside the drawer preserves its focus trap and avoids inert app content.
  const host = trigger.current?.closest(".editor-drawer");

  function close(restoreFocus = false) {
    setOpen(false);
    if (restoreFocus) trigger.current?.focus({ preventScroll: true });
  }
  function choose(index: number) {
    const option = options[index];
    if (!option?.selectable) return;
    onChange(option.device);
    close(true);
  }

  useLayoutEffect(() => {
    if (!open) return;
    function place() {
      const rect = trigger.current?.getBoundingClientRect();
      if (!rect) return;
      const below = window.innerHeight - rect.bottom - 16;
      const above = rect.top - 16;
      const upwards = below < 260 && above > below;
      const height = Math.min(340, Math.max(160, upwards ? above : below));
      const width = Math.min(rect.width, window.innerWidth - 32);
      setPosition({
        top: upwards ? Math.max(8, rect.top - height - 6) : rect.bottom + 6,
        left: Math.max(16, Math.min(rect.left, window.innerWidth - width - 16)),
        width,
        height,
      });
    }
    place();
    window.addEventListener("resize", place);
    window.addEventListener("scroll", place, true);
    return () => {
      window.removeEventListener("resize", place);
      window.removeEventListener("scroll", place, true);
    };
  }, [open]);

  useEffect(() => {
    if (!open) return;
    search.current?.focus({ preventScroll: true });
    function outside(event: Event) {
      const target = event.target as Node;
      if (
        !popup.current?.contains(target) &&
        !trigger.current?.contains(target)
      )
        setOpen(false);
    }
    document.addEventListener("pointerdown", outside);
    document.addEventListener("focusin", outside);
    return () => {
      document.removeEventListener("pointerdown", outside);
      document.removeEventListener("focusin", outside);
    };
  }, [open]);
  useEffect(() => {
    if (disabled) setOpen(false);
  }, [disabled]);
  useEffect(() => {
    if (open)
      popup.current
        ?.querySelector(`#${CSS.escape(`${id}-option-${activeIndex}`)}`)
        ?.scrollIntoView({ block: "nearest" });
  }, [activeIndex, open, id, query]);

  return (
    <div className="device-picker">
      <button
        ref={trigger}
        type="button"
        className="device-picker-trigger"
        disabled={disabled}
        aria-label="绑定设备"
        aria-haspopup="dialog"
        aria-expanded={open}
        aria-controls={open ? `${id}-popup` : undefined}
        onClick={() => {
          setQuery("");
          setActiveId(selectedId);
          setOpen(!open);
        }}
        onKeyDown={(event) => {
          if (event.key === "ArrowDown" || event.key === "ArrowUp") {
            event.preventDefault();
            setQuery("");
            setActiveId(selectedId);
            setOpen(true);
          }
        }}
      >
        <Monitor size={15} />
        <span>{selected?.label || "请选择局域网设备"}</span>
        {selected ? (
          <span
            className={`device-picker-state ${selected.device.online ? "is-online" : ""}`}
          >
            {selected.device.online ? "在线" : "离线"}
          </span>
        ) : null}
        <ChevronDown size={14} />
      </button>
      <p className="device-picker-summary" role="status">
        {loading
          ? "正在探测局域网，保留已有结果…"
          : `检测到 ${all.filter((option) => option.device.online).length} 台在线设备 · ${all.filter((option) => !option.device.online).length} 台离线设备`}
      </p>
      {error ? (
        <p className="device-picker-error" role="status">
          {error}
        </p>
      ) : null}
      {open && host
        ? createPortal(
            <div
              ref={popup}
              id={`${id}-popup`}
              role="dialog"
              aria-label="局域网设备选择"
              className="device-picker-popover"
              style={position}
              onKeyDown={(event) => {
                if (event.key === "Escape") {
                  event.preventDefault();
                  event.stopPropagation();
                  close(true);
                  return;
                }
                if (event.target !== search.current) return;
                const keys = ["ArrowDown", "ArrowUp", "Home", "End", "Enter"];
                if (!keys.includes(event.key)) return;
                event.preventDefault();
                if (event.key === "Enter") {
                  choose(activeIndex);
                  return;
                }
                const next =
                  event.key === "Home"
                    ? 0
                    : event.key === "End"
                      ? options.length - 1
                      : Math.max(
                          0,
                          Math.min(
                            options.length - 1,
                            activeIndex + (event.key === "ArrowDown" ? 1 : -1),
                          ),
                        );
                setActiveId(options[next]?.id || "");
              }}
            >
              <div className="device-picker-search">
                <Search size={15} />
                <input
                  ref={search}
                  role="combobox"
                  aria-label="搜索局域网设备"
                  aria-expanded="true"
                  aria-controls={`${id}-list`}
                  aria-autocomplete="list"
                  aria-activedescendant={
                    options.length ? `${id}-option-${activeIndex}` : undefined
                  }
                  value={query}
                  onChange={(event) => {
                    setQuery(event.target.value);
                    setActiveId("");
                  }}
                  placeholder="搜索设备名称、IP 或 MAC"
                />
                <button
                  type="button"
                  className="icon-action"
                  aria-label="刷新局域网设备"
                  title="刷新局域网设备"
                  disabled={loading}
                  onClick={() => {
                    search.current?.focus();
                    void onRefresh();
                  }}
                >
                  <RefreshCw size={14} className={loading ? "spinning" : ""} />
                </button>
              </div>
              {error ? (
                <p
                  className="device-picker-scan-error"
                  role="status"
                  title={error}
                >
                  扫描失败，保留已有设备。请检查本地网络权限后重试。
                </p>
              ) : null}
              <div
                className="device-picker-options"
                id={`${id}-list`}
                role="listbox"
                aria-label="可检测到的局域网设备"
                aria-busy={loading}
              >
                {options.map((option, index) => (
                  <div
                    key={option.id}
                    id={`${id}-option-${index}`}
                    role="option"
                    aria-selected={option.id === selectedId}
                    aria-disabled={!option.selectable}
                    className={`device-picker-option${index === activeIndex ? " is-active" : ""}${!option.selectable ? " is-unavailable" : ""}`}
                    onPointerMove={() => setActiveId(option.id)}
                    onMouseDown={(event) => event.preventDefault()}
                    onClick={() => choose(index)}
                  >
                    <span className="device-picker-avatar">
                      <Monitor size={17} />
                    </span>
                    <span className="device-picker-identity">
                      <strong>{option.label}</strong>
                      <small>
                        {option.address} ·{" "}
                        {option.device.mac || "缺少 MAC，暂不可绑定"}
                      </small>
                    </span>
                    <span className="device-picker-tags">
                      <span
                        className={`device-picker-state ${option.device.online ? "is-online" : ""}`}
                      >
                        {option.device.online ? "在线" : "离线"}
                      </span>
                      {option.bound ? <span>已绑定</span> : null}
                    </span>
                    <Check
                      size={14}
                      className={
                        option.id === selectedId
                          ? "device-picker-check"
                          : "device-picker-check is-hidden"
                      }
                    />
                  </div>
                ))}
                {!options.length ? (
                  <div className="device-picker-empty">
                    {query
                      ? "没有匹配的设备"
                      : loading
                        ? "正在发现局域网设备…"
                        : "尚未检测到设备"}
                    <small>
                      {query
                        ? "试试名称、IP 或 MAC 地址"
                        : "可点击右上角刷新，或按 MAC 手动添加"}
                    </small>
                  </div>
                ) : null}
              </div>
              <div className="device-picker-footer">
                <span>
                  {loading
                    ? "正在扫描局域网…"
                    : query
                      ? `${options.length} 个匹配结果`
                      : "全部发现设备，不限已绑定"}
                </span>
                <span>↑↓ 选择 · Enter 确认</span>
              </div>
            </div>,
            host,
          )
        : null}
    </div>
  );
}
