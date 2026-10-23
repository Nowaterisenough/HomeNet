import iconUrl from "../../src-tauri/icons/tray-icon.svg";

export default function HomeNetIcon({ size = 22 }: { size?: number }) {
  return (
    <span
      aria-hidden="true"
      style={{
        display: "inline-block",
        flexShrink: 0,
        width: size,
        height: size,
        backgroundColor: "currentColor",
        mask: `url("${iconUrl}") center / contain no-repeat`,
        WebkitMask: `url("${iconUrl}") center / contain no-repeat`,
      }}
    />
  );
}
