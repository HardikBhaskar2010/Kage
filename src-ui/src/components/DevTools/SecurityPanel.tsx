import React from "react";
import "./SecurityPanel.css";
import { ShieldCheck, Lock, Key } from "lucide-react";

export const SecurityPanel: React.FC = () => {
  return (
    <div className="security-panel" role="region" aria-label="Security & Permissions">
      {/* ── Connection & Certificate ────────────────────────────── */}
      <div className="sec-card">
        <div className="sec-card__header">
          <div className="sec-card__icon sec-card__icon--good">
            <Lock size={16} strokeWidth={2} />
          </div>
          <div>
            <h4 className="sec-card__title">Connection is Secure</h4>
            <p className="sec-card__sub">Valid trusted TLS certificate verified by CEF Host</p>
          </div>
          <span className="sec-badge sec-badge--good">TLS 1.3</span>
        </div>

        <div className="sec-grid">
          <div className="sec-kv">
            <span className="sec-k">Subject:</span>
            <span className="sec-v">*.github.com</span>
          </div>
          <div className="sec-kv">
            <span className="sec-k">Issuer:</span>
            <span className="sec-v">DigiCert Global G2 TLS RSA SHA256 2020 CA1</span>
          </div>
          <div className="sec-kv">
            <span className="sec-k">Cipher Suite:</span>
            <span className="sec-v">TLS_AES_128_GCM_SHA256 (128-bit key)</span>
          </div>
          <div className="sec-kv">
            <span className="sec-k">Key Exchange:</span>
            <span className="sec-v">ECDHE with X25519 (253 bits)</span>
          </div>
        </div>
      </div>

      {/* ── 4-Tier Permission Architecture ───────────────────────── */}
      <div className="sec-card">
        <div className="sec-card__header">
          <div className="sec-card__icon sec-card__icon--info">
            <ShieldCheck size={16} strokeWidth={2} />
          </div>
          <div>
            <h4 className="sec-card__title">4-Tier Tool Bus Governance</h4>
            <p className="sec-card__sub">Centralized permission gates enforced in Rust host core</p>
          </div>
        </div>

        <div className="sec-tiers-list">
          <div className="sec-tier-row">
            <div className="sec-tier-badge sec-tier-badge--t1">Tier 1 · Passive</div>
            <div className="sec-tier-desc">
              <strong>Auto-Allowed:</strong> Read-only passive telemetry (DOM snapshot, console stream, network headers). Zero side effects.
            </div>
            <span className="sec-tier-status">Active (Audited)</span>
          </div>

          <div className="sec-tier-row">
            <div className="sec-tier-badge sec-tier-badge--t2">Tier 2 · Mutating</div>
            <div className="sec-tier-desc">
              <strong>Session Scoped:</strong> State-mutating page interactions (click element, type text, scroll viewport). Requires user session grant.
            </div>
            <span className="sec-tier-status sec-tier-status--granted">Granted</span>
          </div>

          <div className="sec-tier-row">
            <div className="sec-tier-badge sec-tier-badge--t3">Tier 3 · High Risk</div>
            <div className="sec-tier-desc">
              <strong>Per-Action Modal:</strong> External requests (network replay, cookie alteration, file download). Requires explicit confirmation.
            </div>
            <span className="sec-tier-status sec-tier-status--prompt">Prompt Per Call</span>
          </div>

          <div className="sec-tier-row">
            <div className="sec-tier-badge sec-tier-badge--t4">Tier 4 · Blocked</div>
            <div className="sec-tier-desc">
              <strong>Hard Forbidden:</strong> Dangerous operations (bypass SSL certs, modify browser binary, arbitrary system shell).
            </div>
            <span className="sec-tier-status sec-tier-status--blocked">Permanently Denied</span>
          </div>
        </div>
      </div>

      {/* ── Secret Redaction & Sanitization ──────────────────────── */}
      <div className="sec-card">
        <div className="sec-card__header">
          <div className="sec-card__icon sec-card__icon--warn">
            <Key size={16} strokeWidth={2} />
          </div>
          <div>
            <h4 className="sec-card__title">Context Sanitizer & Secret Redaction</h4>
            <p className="sec-card__sub">Automatic stripping of credentials before LLM dispatch</p>
          </div>
        </div>

        <div className="sec-grid">
          <div className="sec-kv">
            <span className="sec-k">Bearer Tokens:</span>
            <span className="sec-v sec-v--good">Auto-Redacted (&lt;REDACTED_BEARER&gt;)</span>
          </div>
          <div className="sec-kv">
            <span className="sec-k">Cookie Values:</span>
            <span className="sec-v sec-v--good">Masked (&lt;REDACTED_COOKIE&gt;)</span>
          </div>
          <div className="sec-kv">
            <span className="sec-k">Input Passwords:</span>
            <span className="sec-v sec-v--good">Zeroed (&lt;REDACTED_PASSWORD&gt;)</span>
          </div>
          <div className="sec-kv">
            <span className="sec-k">Audit Log:</span>
            <span className="sec-v">Immutable SQLite security_audit.db</span>
          </div>
        </div>
      </div>
    </div>
  );
};
