import React, { useEffect } from "react";
import { AlertTriangle, ShieldAlert, CheckCircle, XCircle } from "lucide-react";
import "./ProfileEscalationModal.css";

interface ProfileEscalationModalProps {
  isOpen: boolean;
  tabId: string;
  targetProfileId: string;
  reason: string;
  onConfirm: () => Promise<boolean>;
  onCancel: () => Promise<void>;
}

export const ProfileEscalationModal: React.FC<ProfileEscalationModalProps> = ({
  isOpen,
  tabId,
  targetProfileId,
  reason,
  onConfirm,
  onCancel,
}) => {
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === "Escape" && isOpen) {
        void onCancel();
      }
    };
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [isOpen, onCancel]);

  if (!isOpen) return null;

  return (
    <div className="escalation-modal-backdrop" role="alertdialog" aria-modal="true" aria-labelledby="esc-title">
      <div className="escalation-modal">
        <div className="escalation-modal__badge">
          <ShieldAlert size={14} /> Tier 3 Privilege Escalation (INV-06)
        </div>

        <h2 id="esc-title" className="escalation-modal__title">
          Agent Shared Session Authorization
        </h2>

        <p className="escalation-modal__description">
          An autonomous agent workflow has requested to escalate from its clean sandbox to access your personal or work profile session.
        </p>

        <div className="escalation-modal__highlight-box">
          <div className="escalation-modal__field">
            <span className="escalation-modal__field-label">Target Profile:</span>
            <span className="escalation-modal__field-value">{targetProfileId}</span>
          </div>
          <div className="escalation-modal__field">
            <span className="escalation-modal__field-label">Target Tab:</span>
            <span className="escalation-modal__field-value">{tabId}</span>
          </div>
          <div className="escalation-modal__field">
            <span className="escalation-modal__field-label">Declared Reason:</span>
            <span className="escalation-modal__field-value">{reason}</span>
          </div>
        </div>

        <div className="escalation-modal__warning">
          <AlertTriangle size={18} style={{ flexShrink: 0, marginTop: 1 }} />
          <div>
            <strong>Security Notice:</strong> Authorizing this request grants the agent access to all active session cookies, authenticated storage tokens, and saved credentials within the <code>{targetProfileId}</code> profile. This operation is committed to the immutable SHA-256 audit ledger.
          </div>
        </div>

        <div className="escalation-modal__actions">
          <button
            type="button"
            className="escalation-btn escalation-btn--deny"
            onClick={() => void onCancel()}
            id="btn-deny-escalation"
          >
            <XCircle size={15} /> Deny (Fail Closed)
          </button>
          <button
            type="button"
            className="escalation-btn escalation-btn--approve"
            onClick={() => void onConfirm()}
            id="btn-approve-escalation"
          >
            <CheckCircle size={15} /> Authorize Escalation
          </button>
        </div>
      </div>
    </div>
  );
};
