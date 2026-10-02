import React, { useState, useEffect, useCallback } from "react";
import { Plus, Check } from "lucide-react";
import type {
  ProfileMetadata,
  PermissionType,
  PermissionDecision,
} from "../../ipc/client";
import { setPermission } from "../../ipc/client";
import "./SitePermissionsPanel.css";

interface SitePermissionsPanelProps {
  profiles: ProfileMetadata[];
  activeProfileId?: string;
}

interface LocalRule {
  id: string;
  origin: string;
  permission: PermissionType;
  decision: PermissionDecision;
  updatedAt: string;
}

const DEFAULT_SAMPLE_RULES: LocalRule[] = [
  {
    id: "r-1",
    origin: "https://github.com",
    permission: "clipboard_read",
    decision: "allow",
    updatedAt: "Just now",
  },
  {
    id: "r-2",
    origin: "https://unknown-ad-tracker.io",
    permission: "geolocation",
    decision: "deny",
    updatedAt: "Today",
  },
  {
    id: "r-3",
    origin: "https://meet.google.com",
    permission: "camera",
    decision: "prompt",
    updatedAt: "Yesterday",
  },
  {
    id: "r-4",
    origin: "https://meet.google.com",
    permission: "microphone",
    decision: "prompt",
    updatedAt: "Yesterday",
  },
];

const PERMISSION_OPTIONS: { label: string; value: PermissionType }[] = [
  { label: "Geolocation", value: "geolocation" },
  { label: "Notifications", value: "notifications" },
  { label: "Camera", value: "camera" },
  { label: "Microphone", value: "microphone" },
  { label: "Clipboard Read", value: "clipboard_read" },
  { label: "Clipboard Write", value: "clipboard_write" },
  { label: "Downloads", value: "downloads" },
  { label: "Popups", value: "popups" },
];

export const SitePermissionsPanel: React.FC<SitePermissionsPanelProps> = ({
  profiles,
  activeProfileId = "personal",
}) => {
  const [selectedProfileId, setSelectedProfileId] = useState<string>(activeProfileId);
  const [rules, setRules] = useState<LocalRule[]>(DEFAULT_SAMPLE_RULES);
  const [newOrigin, setNewOrigin] = useState("");
  const [newPermission, setNewPermission] = useState<PermissionType>("geolocation");
  const [newDecision, setNewDecision] = useState<PermissionDecision>("prompt");
  const [isSaving, setIsSaving] = useState(false);
  const [feedbackMsg, setFeedbackMsg] = useState<string | null>(null);

  useEffect(() => {
    if (activeProfileId) {
      setSelectedProfileId(activeProfileId);
    }
  }, [activeProfileId]);

  const handleSaveRule = useCallback(async () => {
    let cleanOrigin = newOrigin.trim();
    if (!cleanOrigin) return;

    if (!cleanOrigin.startsWith("http://") && !cleanOrigin.startsWith("https://")) {
      cleanOrigin = `https://${cleanOrigin}`;
    }

    setIsSaving(true);
    try {
      await setPermission(selectedProfileId, cleanOrigin, newPermission, newDecision);
      const newRule: LocalRule = {
        id: `rule-${Date.now()}`,
        origin: cleanOrigin,
        permission: newPermission,
        decision: newDecision,
        updatedAt: "Just now",
      };

      setRules((prev) => [
        newRule,
        ...prev.filter(
          (r) => !(r.origin === cleanOrigin && r.permission === newPermission)
        ),
      ]);
      setNewOrigin("");
      setFeedbackMsg("Permission rule saved to profile matrix.");
      setTimeout(() => setFeedbackMsg(null), 2500);
    } catch (err) {
      console.warn("Could not save permission rule to host:", err);
      // Fallback update in UI for offline / mock mode
      const newRule: LocalRule = {
        id: `rule-${Date.now()}`,
        origin: cleanOrigin,
        permission: newPermission,
        decision: newDecision,
        updatedAt: "Local mock",
      };
      setRules((prev) => [
        newRule,
        ...prev.filter(
          (r) => !(r.origin === cleanOrigin && r.permission === newPermission)
        ),
      ]);
      setNewOrigin("");
    } finally {
      setIsSaving(false);
    }
  }, [newOrigin, newPermission, newDecision, selectedProfileId]);

  const handleDecisionToggle = async (rule: LocalRule) => {
    const nextDecision: PermissionDecision =
      rule.decision === "allow"
        ? "prompt"
        : rule.decision === "prompt"
        ? "deny"
        : "allow";

    try {
      await setPermission(selectedProfileId, rule.origin, rule.permission, nextDecision);
    } catch (e) {
      console.warn("Mock updating permission decision:", e);
    }

    setRules((prev) =>
      prev.map((r) =>
        r.id === rule.id ? { ...r, decision: nextDecision, updatedAt: "Just now" } : r
      )
    );
  };

  return (
    <div className="permissions-panel">
      <div className="permissions-panel__header">
        <div>
          <h3 style={{ margin: "0 0 4px 0", fontSize: "14px", fontWeight: 600 }}>
            Site Permissions & Capabilities
          </h3>
          <p style={{ margin: 0, fontSize: "12px", opacity: 0.7 }}>
            Origin-scoped capability policies governed per profile (INV-04, INV-10).
          </p>
        </div>

        <div className="permissions-panel__profile-select">
          <label style={{ fontSize: "12px", opacity: 0.8 }} htmlFor="profile-select-dropdown">
            Profile:
          </label>
          <select
            id="profile-select-dropdown"
            className="permissions-select"
            value={selectedProfileId}
            onChange={(e) => setSelectedProfileId(e.target.value)}
          >
            {profiles.map((p) => (
              <option key={p.id} value={p.id}>
                {p.name} ({p.kind})
              </option>
            ))}
          </select>
        </div>
      </div>

      {feedbackMsg && (
        <div
          style={{
            fontSize: "12px",
            color: "#78d4a0",
            background: "rgba(120, 212, 160, 0.12)",
            padding: "6px 12px",
            borderRadius: "6px",
            border: "1px solid rgba(120, 212, 160, 0.25)",
            display: "flex",
            alignItems: "center",
            gap: "6px",
          }}
        >
          <Check size={14} />
          {feedbackMsg}
        </div>
      )}

      {/* Add New Rule Card */}
      <div className="permissions-add-card">
        <span style={{ fontSize: "12px", fontWeight: 600, color: "var(--color-peach, #F9DBBD)" }}>
          Define Origin Override
        </span>
        <div className="permissions-add-row">
          <input
            type="text"
            className="permissions-input"
            placeholder="e.g. https://app.example.com"
            value={newOrigin}
            onChange={(e) => setNewOrigin(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter") void handleSaveRule();
            }}
          />
          <select
            className="permissions-select"
            value={newPermission}
            onChange={(e) => setNewPermission(e.target.value as PermissionType)}
          >
            {PERMISSION_OPTIONS.map((opt) => (
              <option key={opt.value} value={opt.value}>
                {opt.label}
              </option>
            ))}
          </select>
          <select
            className="permissions-select"
            value={newDecision}
            onChange={(e) => setNewDecision(e.target.value as PermissionDecision)}
          >
            <option value="allow">Allow</option>
            <option value="prompt">Prompt</option>
            <option value="deny">Deny</option>
          </select>
          <button
            type="button"
            className="permissions-btn"
            onClick={() => void handleSaveRule()}
            disabled={!newOrigin.trim() || isSaving}
          >
            <Plus size={13} style={{ verticalAlign: "middle", marginRight: "4px" }} />
            {isSaving ? "Saving..." : "Add Rule"}
          </button>
        </div>
      </div>

      {/* Rules Table */}
      <div className="permissions-table-wrap">
        <table className="permissions-table">
          <thead>
            <tr>
              <th>Origin</th>
              <th>Permission</th>
              <th>Decision (Click to Toggle)</th>
              <th>Updated</th>
            </tr>
          </thead>
          <tbody>
            {rules.length === 0 ? (
              <tr>
                <td colSpan={4} style={{ textAlign: "center", opacity: 0.6, padding: "20px" }}>
                  No origin overrides configured for this profile.
                </td>
              </tr>
            ) : (
              rules.map((rule) => (
                <tr key={rule.id}>
                  <td style={{ fontFamily: "var(--font-mono, monospace)" }}>{rule.origin}</td>
                  <td style={{ textTransform: "capitalize" }}>{rule.permission.replace("_", " ")}</td>
                  <td>
                    <button
                      type="button"
                      onClick={() => void handleDecisionToggle(rule)}
                      style={{ background: "none", border: "none", padding: 0, cursor: "pointer" }}
                      title="Click to cycle decision (Allow -> Prompt -> Deny)"
                    >
                      <span className={`permission-badge permission-badge--${rule.decision}`}>
                        {rule.decision}
                      </span>
                    </button>
                  </td>
                  <td style={{ opacity: 0.6 }}>{rule.updatedAt}</td>
                </tr>
              ))
            )}
          </tbody>
        </table>
      </div>
    </div>
  );
};
