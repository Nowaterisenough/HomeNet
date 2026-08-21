import { Download, ShieldCheck, TriangleAlert } from "lucide-react";
import type { AppUpdateResult } from "../types";

interface UpdateConfirmDialogProps {
  update: AppUpdateResult;
  onCancel: () => void;
  onConfirm: () => void;
}

export default function UpdateConfirmDialog({
  update,
  onCancel,
  onConfirm,
}: UpdateConfirmDialogProps) {
  const signed = update.verification === "signed";
  const notes = update.notes.trim();
  const notesPreview = notes.length > 600 ? `${notes.slice(0, 600)}…` : notes;

  return (
    <div
      className="modal-backdrop update-confirm-backdrop"
      onClick={(event) => event.currentTarget === event.target && onCancel()}
    >
      <section
        className="modal-dialog update-confirm-modal"
        role="dialog"
        aria-modal="true"
        aria-labelledby="update-confirm-title"
      >
        <header className="modal-header">
          <h2 id="update-confirm-title">安装 HomeNet {update.latest_version}</h2>
        </header>

        <div className="update-confirm-content">
          <div className={`update-verification ${signed ? "is-signed" : "is-unsigned"}`}>
            {signed ? (
              <ShieldCheck size={18} strokeWidth={2.1} aria-hidden="true" />
            ) : (
              <TriangleAlert size={18} strokeWidth={2.1} aria-hidden="true" />
            )}
            <div>
              <strong>{signed ? "签名更新" : "GitHub HTTPS 更新"}</strong>
              <p>
                {signed
                  ? "更新包将通过内置公钥验证签名后安装。"
                  : "此版本未配置 updater 签名，将从 GitHub HTTPS 下载；macOS 安装后会解除应用隔离属性。"}
              </p>
            </div>
          </div>

          <div className="update-release-notes">
            <h3>更新说明</h3>
            <p>{notesPreview || "该版本未提供更新说明。"}</p>
          </div>
        </div>

        <footer className="update-confirm-actions">
          <button className="btn btn-secondary" type="button" onClick={onCancel}>
            取消
          </button>
          <button className="btn btn-primary" type="button" autoFocus onClick={onConfirm}>
            <Download size={13} strokeWidth={2.2} aria-hidden="true" />
            下载并安装
          </button>
        </footer>
      </section>
    </div>
  );
}
