import { Binary, Globe, Monitor, Network, Server, ShieldCheck, Workflow } from "lucide-react";
import type { LucideIcon } from "lucide-react";

interface StatusCardProps {
  title: string;
  value: string;
  subtitle: string;
  status: "normal" | "warning" | "error" | "success";
  icon: string;
}

const iconMap: Record<string, LucideIcon> = {
  globe: Globe,
  ipv6: Binary,
  shield: ShieldCheck,
  rules: Workflow,
  devices: Monitor,
  proxy: Server,
};

export default function StatusCard({ title, value, subtitle, status, icon }: StatusCardProps) {
  const Icon = iconMap[icon] ?? Network;

  return (
    <article className={`status-card status-${status} card-${icon}`}>
      <div className={`icon-disc icon-${icon}`} aria-hidden="true">
        <Icon size={28} strokeWidth={2.45} />
      </div>

      <div className="card-copy">
        <h3>{title}</h3>
        <p className="card-value">{value}</p>
        <p className="card-subtitle">{subtitle}</p>
      </div>
    </article>
  );
}
