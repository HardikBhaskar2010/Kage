import React, { useState, useEffect } from "react";
import { X, Plus, Shield, Trash2, ExternalLink } from "lucide-react";
import type { ProfileMetadata, ProfileKind } from "../../ipc/client";
import "./ProfileSwitcherModal.css";

interface ProfileSwitcherModalProps {
  isOpen: boolean;
  onClose: () => void;
  profiles: ProfileMetadata[];
  activeProfileId?: string;
  onSelectProfile: (profileId: string) => void;
  onOpenTabInProfile: (profileId: string) => void;
  onCreateProfile: (name: string, kind: ProfileKind, color?: string, icon?: string) => Promise<void>;
  onDeleteProfile: (profileId: string) => Promise<void>;
}

const SWATCH_COLORS = ["#3B82F6", "#8B5CF6", "#EC4899", "#10B981", "#F59E0B", "#EF4444", "#06B6D4"];

export const ProfileSwitcherModal: React.FC<ProfileSwitcherModalProps> = ({
  isOpen,
  onClose,
  profiles,
  activeProfileId,
  onSelectProfile,
  onOpenTabInProfile,
  onCreateProfile,
  onDeleteProfile,
}) => {
  const [newName, setNewName] = useState("");
  const [newKind, setNewKind] = useState<ProfileKind>("personal");
  const [selectedColor, setSelectedColor] = useState(SWATCH_COLORS[0]);
  const [isSubmitting, setIsSubmitting] = useState(false);

  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === "Escape" && isOpen) {
        onClose();
      }
    };
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [isOpen, onClose]);

  if (!isOpen) return null;

  const handleCreate = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!newName.trim() || isSubmitting) return;

    setIsSubmitting(true);
    try {
      await onCreateProfile(newName.trim(), newKind, selectedColor, "user");
      setNewName("");
    } finally {
      setIsSubmitting(false);
    }
  };

  const handleLaunchSandbox = async () => {
    onOpenTabInProfile("agent_sandbox");
    onClose();
  };

  return (
    <div className="profile-modal-backdrop" onClick={onClose} role="dialog" aria-modal="true">
      <div className="profile-modal" onClick={(e) => e.stopPropagation()}>
        <div className="profile-modal__header">
          <h2 className="profile-modal__title">Profiles & Identity</h2>
          <button
            type="button"
            className="profile-modal__close-btn"
            onClick={onClose}
            aria-label="Close modal"
          >
            <X size={16} />
          </button>
        </div>

        <div className="profile-list">
          {profiles.map((p) => {
            const isActive = p.id === activeProfileId;
            const isPersonal = p.id === "personal";

            return (
              <div
                key={p.id}
                className={`profile-item ${isActive ? "profile-item--active" : ""}`}
              >
                <div className="profile-item__left">
                  <span
                    className="profile-item__color-dot"
                    style={{ backgroundColor: p.color, color: p.color }}
                  />
                  <div className="profile-item__info">
                    <span className="profile-item__name">{p.name}</span>
                    <span className="profile-item__meta">
                      {p.kind}
                      {p.is_ephemeral && (
                        <span className="profile-item__badge">Ephemeral Wipe</span>
                      )}
                    </span>
                  </div>
                </div>

                <div className="profile-item__actions">
                  {!isActive && (
                    <button
                      type="button"
                      className="profile-action-btn"
                      onClick={() => {
                        onSelectProfile(p.id);
                        onClose();
                      }}
                      title="Switch active tab to this profile"
                    >
                      Switch
                    </button>
                  )}
                  <button
                    type="button"
                    className="profile-action-btn"
                    onClick={() => {
                      onOpenTabInProfile(p.id);
                      onClose();
                    }}
                    title="Open new tab in this profile"
                  >
                    <ExternalLink size={11} /> New Tab
                  </button>
                  {!isPersonal && (
                    <button
                      type="button"
                      className="profile-action-btn profile-action-btn--delete"
                      onClick={() => onDeleteProfile(p.id)}
                      title="Delete profile"
                      aria-label={`Delete ${p.name} profile`}
                    >
                      <Trash2 size={11} />
                    </button>
                  )}
                </div>
              </div>
            );
          })}
        </div>

        <div className="profile-quick-actions">
          <button
            type="button"
            className="profile-quick-btn"
            onClick={handleLaunchSandbox}
            id="btn-launch-sandbox"
          >
            <Shield size={13} strokeWidth={2} /> Launch Ephemeral Sandbox
          </button>
        </div>

        <div className="profile-modal__create-section">
          <div className="profile-modal__section-title">Create Custom Profile</div>
          <form className="profile-create-form" onSubmit={handleCreate}>
            <div className="profile-input-row">
              <input
                type="text"
                className="profile-input"
                placeholder="Profile Name (e.g. Research, Staging)"
                value={newName}
                onChange={(e) => setNewName(e.target.value)}
                required
              />
              <select
                className="profile-select"
                value={newKind}
                onChange={(e) => setNewKind(e.target.value as ProfileKind)}
              >
                <option value="personal">Personal</option>
                <option value="work">Work</option>
                <option value="temporary">Temporary</option>
              </select>
            </div>

            <div className="profile-color-picker">
              <span style={{ fontSize: 11, color: "var(--color-text-secondary)" }}>Color:</span>
              {SWATCH_COLORS.map((c) => (
                <button
                  type="button"
                  key={c}
                  className={`profile-color-swatch ${selectedColor === c ? "profile-color-swatch--selected" : ""}`}
                  style={{ backgroundColor: c }}
                  onClick={() => setSelectedColor(c)}
                  aria-label={`Select color ${c}`}
                />
              ))}
              <div style={{ flex: 1 }} />
              <button
                type="submit"
                className="profile-action-btn profile-action-btn--primary"
                disabled={!newName.trim() || isSubmitting}
              >
                <Plus size={12} /> Add Profile
              </button>
            </div>
          </form>
        </div>
      </div>
    </div>
  );
};
