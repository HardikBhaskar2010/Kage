import React, { useState } from "react";
import "./SettingsPage.css";
import { useBrowser } from "../../context/BrowserContext";
import {
  Palette,
  Bot,
  Shield,
  Info,
  Check,
  Eye,
  EyeOff,
  ExternalLink,
  Save,
} from "lucide-react";
import { aiProviderRegistry } from "../../services/aiProviderDiscovery";
import { SpotlightCard, ShimmerButton, BorderBeam } from "../ui";

export const SettingsPage: React.FC = () => {
  const {
    accentTheme,
    setAccentTheme,
    glassOpacity,
    setGlassOpacity,
    adBlockerEnabled,
    setAdBlockerEnabled,
    aiModel,
    setAiModel,
  } = useBrowser();

  const [activeTab, setActiveTab] = useState<"appearance" | "ai" | "privacy" | "about">("appearance");
  const [apiKey, setApiKey] = useState("sk-ant-api03-sample99824...");
  const [showKey, setShowKey] = useState(false);
  const [maxSteps, setMaxSteps] = useState(10);
  const [savedAlert, setSavedAlert] = useState(false);

  const handleSave = () => {
    setSavedAlert(true);
    setTimeout(() => setSavedAlert(false), 1500);
  };

  return (
    <div className="settings-page" role="main" aria-label="KAGE Settings">
      <div className="settings-container">
        {/* Header */}
        <div className="settings-header">
          <h1 className="settings-title">Preferences</h1>
          <p className="settings-subtitle">Manage browser aesthetics, AI intelligence keys, and security shields.</p>
        </div>

        {/* Layout: Left Sidebar + Right Content */}
        <div className="settings-body">
          <nav className="settings-nav" aria-label="Settings Categories">
            <button
              className={`settings-nav-btn ${activeTab === "appearance" ? "settings-nav-btn--active" : ""}`}
              onClick={() => setActiveTab("appearance")}
            >
              <Palette size={16} strokeWidth={1.8} />
              Appearance
            </button>
            <button
              className={`settings-nav-btn ${activeTab === "ai" ? "settings-nav-btn--active" : ""}`}
              onClick={() => setActiveTab("ai")}
            >
              <Bot size={16} strokeWidth={1.8} />
              AI Copilot
            </button>
            <button
              className={`settings-nav-btn ${activeTab === "privacy" ? "settings-nav-btn--active" : ""}`}
              onClick={() => setActiveTab("privacy")}
            >
              <Shield size={16} strokeWidth={1.8} />
              Privacy & Shields
            </button>
            <button
              className={`settings-nav-btn ${activeTab === "about" ? "settings-nav-btn--active" : ""}`}
              onClick={() => setActiveTab("about")}
            >
              <Info size={16} strokeWidth={1.8} />
              About KAGE
            </button>
          </nav>

          <div className="settings-content">
            {/* ── Appearance Tab ──────────────────────────────────── */}
            {activeTab === "appearance" && (
              <div className="settings-section">
                <h2>Liquid Glass & Theming</h2>

                <div className="settings-group">
                  <label className="settings-label">Color Theme Accent</label>
                  <p className="settings-desc">Select the primary accent gradient applied across chrome elements.</p>
                  <div className="theme-options">
                    {[
                      { id: "peach", label: "Peach & Wine (Default)", color: "#F9DBBD", sub: "#450920" },
                      { id: "violet", label: "Cyber Violet", color: "#C084FC", sub: "#2E1065" },
                      { id: "frost", label: "Obsidian Frost", color: "#38BDF8", sub: "#082F49" },
                    ].map((t) => (
                      <button
                        key={t.id}
                        type="button"
                        className={`theme-pill ${accentTheme === t.id ? "theme-pill--selected" : ""}`}
                        onClick={() => {
                          setAccentTheme(t.id as any);
                          handleSave();
                        }}
                      >
                        <span className="theme-dot" style={{ background: `linear-gradient(135deg, ${t.color}, ${t.sub})` }} />
                        <span>{t.label}</span>
                        {accentTheme === t.id && <Check size={14} strokeWidth={2} className="theme-check" />}
                      </button>
                    ))}
                  </div>
                </div>

                <div className="settings-group">
                  <div className="settings-row-between">
                    <div>
                      <label className="settings-label">Glass Surface Opacity</label>
                      <p className="settings-desc">Controls transparency level for tabs, drawers, and omnibox.</p>
                    </div>
                    <span className="settings-badge">{Math.round(glassOpacity * 100)}%</span>
                  </div>
                  <input
                    type="range"
                    min="0.5"
                    max="0.98"
                    step="0.02"
                    value={glassOpacity}
                    onChange={(e) => setGlassOpacity(parseFloat(e.target.value))}
                    className="settings-slider"
                  />
                </div>
              </div>
            )}

            {/* ── AI Copilot Tab ──────────────────────────────────── */}
            {activeTab === "ai" && (
              <div className="settings-section">
                <h2>AI Subsystem & Dynamic Capability Routing</h2>

                <div className="settings-group">
                  <div className="settings-row-between">
                    <div>
                      <label className="settings-label">Autonomous Model Routing Policy</label>
                      <p className="settings-desc">
                        Models are dynamically discovered and validated against their capabilities (Tool Bus, Streaming, Vision, Context) rather than static hardcoding.
                      </p>
                    </div>
                  </div>

                  <div className="model-radios">
                    {aiProviderRegistry.getProviders().map((provider) => (
                      <div key={provider.id} className="provider-block">
                        <div className="provider-header">
                          <span className="provider-name">{provider.name}</span>
                          <span className={`provider-status provider-status--${provider.status}`}>
                            {provider.status.toUpperCase()}
                          </span>
                        </div>

                        {provider.models.map((m) => (
                          <label
                            key={m.id}
                            className={`model-card ${aiModel === m.id ? "model-card--selected" : ""}`}
                          >
                            <input
                              type="radio"
                              name="aiModel"
                              value={m.id}
                              checked={aiModel === m.id}
                              onChange={() => {
                                setAiModel(m.id);
                                aiProviderRegistry.setActiveModel(m.id);
                                handleSave();
                              }}
                            />
                            <div className="model-info">
                              <strong>{m.name}</strong>
                              <span>{m.tagline}</span>
                              <div className="model-chips">
                                {m.capabilities.streaming && (
                                  <span className="model-chip">STREAMING</span>
                                )}
                                {m.capabilities.toolCalling && (
                                  <span className="model-chip model-chip--tier">TOOL BUS READY</span>
                                )}
                                {m.capabilities.vision && (
                                  <span className="model-chip model-chip--vision">VISION</span>
                                )}
                                <span className="model-chip">
                                  {Math.round(m.capabilities.contextWindow / 1000)}k CONTEXT
                                </span>
                                <span className="model-chip model-chip--speed">
                                  {m.latencyP95Ms}ms P95
                                </span>
                              </div>
                            </div>
                          </label>
                        ))}
                      </div>
                    ))}
                  </div>
                </div>

                <div className="settings-group">
                  <label className="settings-label">API Key Override</label>
                  <p className="settings-desc">Bring your own key for direct multi-provider streaming.</p>
                  <div className="api-key-box">
                    <input
                      type={showKey ? "text" : "password"}
                      value={apiKey}
                      onChange={(e) => setApiKey(e.target.value)}
                      className="api-key-input"
                    />
                    <button
                      type="button"
                      className="api-key-toggle"
                      onClick={() => setShowKey(!showKey)}
                    >
                      {showKey ? <EyeOff size={15} /> : <Eye size={15} />}
                    </button>
                  </div>
                </div>

                <div className="settings-group">
                  <div className="settings-row-between">
                    <div>
                      <label className="settings-label">Maximum Autonomous Steps</label>
                      <p className="settings-desc">Bounded agent loop cap per query (Prime Architectural Invariant).</p>
                    </div>
                    <span className="settings-badge">{maxSteps} Steps</span>
                  </div>
                  <input
                    type="range"
                    min="1"
                    max="10"
                    value={maxSteps}
                    onChange={(e) => setMaxSteps(parseInt(e.target.value))}
                    className="settings-slider"
                  />
                </div>
              </div>
            )}

            {/* ── Privacy & Shields Tab ───────────────────────────── */}
            {activeTab === "privacy" && (
              <div className="settings-section">
                <h2>Privacy, Ad Shield & Security</h2>

                <div className="settings-toggle-row">
                  <div>
                    <strong className="toggle-title">Block Third-Party Trackers & Fingerprinting</strong>
                    <p className="settings-desc">Partitions third-party state and blocks known tracking domains.</p>
                  </div>
                  <button
                    type="button"
                    className={`ext-switch ${adBlockerEnabled ? "ext-switch--on" : ""}`}
                    onClick={() => {
                      setAdBlockerEnabled(!adBlockerEnabled);
                      handleSave();
                    }}
                    role="switch"
                    aria-checked={adBlockerEnabled}
                  >
                    <span className="ext-switch__thumb" />
                  </button>
                </div>

                <div className="settings-toggle-row">
                  <div>
                    <strong className="toggle-title">Strict Delimiting & Prompt Injection Shield</strong>
                    <p className="settings-desc">Wraps untrusted DOM text in isolation tags before dispatch to model.</p>
                  </div>
                  <span className="sec-tier-status sec-tier-status--granted">Always Enforced</span>
                </div>

                <div className="settings-toggle-row">
                  <div>
                    <strong className="toggle-title">Automatic Secret Redaction</strong>
                    <p className="settings-desc">Strips Authorization headers, API keys, and session cookies from context.</p>
                  </div>
                  <span className="sec-tier-status sec-tier-status--granted">Active</span>
                </div>
              </div>
            )}

            {/* ── About KAGE Tab ──────────────────────────────────── */}
            {activeTab === "about" && (
              <div className="settings-section">
                <h2>About KAGE (影)</h2>
                <SpotlightCard className="about-card" style={{ position: "relative", overflow: "hidden" }}>
                  <BorderBeam size={220} duration={12} colorFrom="#F9DBBD" colorTo="#DA627D" />
                  <div className="about-logo-row">
                    <img src="/Logo.png" alt="KAGE" className="about-logo" />
                    <div>
                      <h3>KAGE Browser</h3>
                      <p>Version 0.2.1-dev (Architectural Milestone 1.0.0)</p>
                    </div>
                  </div>

                  <div className="about-specs">
                    <div className="about-spec-row">
                      <span>Host Engine:</span>
                      <strong>Tauri 2.x / Rust 1.80+</strong>
                    </div>
                    <div className="about-spec-row">
                      <span>Browser Runtime:</span>
                      <strong>Chromium Embedded Framework (CEF v126)</strong>
                    </div>
                    <div className="about-spec-row">
                      <span>UI Chrome:</span>
                      <strong>React 18+ · Liquid Glass Design System</strong>
                    </div>
                    <div className="about-spec-row">
                      <span>Audit Trail:</span>
                      <strong>SQLite 3 (`security_audit.db`)</strong>
                    </div>
                  </div>

                  <div className="about-links">
                    <a
                      href="https://github.com/HardikBhaskar2010/Kage"
                      target="_blank"
                      rel="noreferrer"
                      className="about-link"
                    >
                      <ExternalLink size={13} />
                      GitHub Repository
                    </a>
                  </div>
                </SpotlightCard>
              </div>
            )}

            {/* Save Preferences Action */}
            <div style={{ marginTop: "24px", display: "flex", justifyContent: "flex-end" }}>
              <ShimmerButton
                variant="primary"
                size="md"
                icon={<Save size={15} />}
                onClick={handleSave}
              >
                Save Preferences
              </ShimmerButton>
            </div>

            {/* Notification alert on save */}
            {savedAlert && (
              <div className="settings-toast">
                <Check size={14} strokeWidth={2} />
                <span>Preference updated</span>
              </div>
            )}
          </div>
        </div>
      </div>
    </div>
  );
};
