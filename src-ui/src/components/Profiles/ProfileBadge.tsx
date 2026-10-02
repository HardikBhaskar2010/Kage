import React from "react";
import type { ProfileMetadata } from "../../ipc/client";
import "./ProfileBadge.css";

interface ProfileBadgeProps {
  profile?: ProfileMetadata;
  onClick?: () => void;
}

export const ProfileBadge: React.FC<ProfileBadgeProps> = ({ profile, onClick }) => {
  const name = profile?.name || "Personal";
  const color = profile?.color || "#3B82F6";
  const kind = profile?.kind || "personal";

  return (
    <button
      type="button"
      className="profile-badge"
      onClick={onClick}
      aria-label={`Current Profile: ${name}. Click to switch profiles.`}
      id="profile-badge-btn"
    >
      <span
        className="profile-badge__indicator"
        style={{ backgroundColor: color, color }}
        aria-hidden="true"
      />
      <span className="profile-badge__label">{name}</span>
      {profile?.is_ephemeral && (
        <span className="profile-badge__tag">
          {kind === "agent_sandbox" ? "Sandbox" : "Temp"}
        </span>
      )}
    </button>
  );
};
