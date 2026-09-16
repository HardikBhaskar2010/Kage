import React from "react";
import "./BrowserWebview.css";
import { useBrowser } from "../../context/BrowserContext";
import { ExternalLink, ShieldCheck, RefreshCw, GitBranch, Star, Eye } from "lucide-react";

interface BrowserWebviewProps {
  url: string;
}

export const BrowserWebview: React.FC<BrowserWebviewProps> = ({ url }) => {
  const { activeTab, refresh } = useBrowser();

  const isGitHub = url.includes("github.com");
  const isVercel = url.includes("vercel.com");
  const isDocs = url.includes("docs") || url.includes("nextjs.org");

  return (
    <div className="browser-webview" role="region" aria-label="Web page viewport">
      {/* ── Top Loading Bar ─────────────────────────────────────── */}
      {activeTab?.isLoading && <div className="webview-loading-bar" />}

      {/* ── Page Content Simulator ──────────────────────────────── */}
      <div className="webview-stage">
        {/* Page Top Nav / Simulation Banner */}
        <div className="webview-site-header">
          <div className="webview-site-brand">
            <span className="webview-site-pill">
              <ShieldCheck size={13} strokeWidth={2} />
              Verified HTTPS
            </span>
            <span className="webview-site-host">{url}</span>
          </div>
          <div className="webview-site-actions">
            <button className="webview-site-btn" onClick={refresh} title="Reload page">
              <RefreshCw size={12} strokeWidth={2} />
            </button>
            <a
              href={url}
              target="_blank"
              rel="noreferrer"
              className="webview-site-btn"
              title="Open in default system browser"
            >
              <ExternalLink size={12} strokeWidth={2} />
            </a>
          </div>
        </div>

        {/* Dynamic Interactive Page Body */}
        <div className="webview-page-body">
          {isGitHub ? (
            <div className="sim-github">
              <div className="sim-gh-repo-header">
                <div className="sim-gh-title-row">
                  <h1 className="sim-gh-repo-name">HardikBhaskar2010 / <strong>Kage</strong></h1>
                  <span className="sim-gh-badge">Public</span>
                </div>
                <div className="sim-gh-stats">
                  <button className="sim-gh-btn"><Eye size={12} /> Watch <span>12</span></button>
                  <button className="sim-gh-btn"><GitBranch size={12} /> Fork <span>2</span></button>
                  <button className="sim-gh-btn"><Star size={12} /> Star <span>48</span></button>
                </div>
              </div>

              <div className="sim-gh-branch-bar">
                <span className="sim-gh-branch"><GitBranch size={13} /> main</span>
                <span className="sim-gh-commit">Latest commit: <code>feat: implement liquid-glass design system v0.2.1</code></span>
              </div>

              <div className="sim-gh-file-list">
                {[
                  { name: "crates/kage-core", type: "dir", msg: "core ToolBus permission pipeline", time: "2 hours ago" },
                  { name: "crates/kage-cdp", type: "dir", msg: "WebSocket CDP client & broker", time: "3 hours ago" },
                  { name: "src-ui", type: "dir", msg: "Liquid Glass UI Chrome & DevTools", time: "just now" },
                  { name: "AGENTS.md", type: "file", msg: "Autonomous agent operating guidelines", time: "1 hour ago" },
                  { name: "Cargo.toml", type: "file", msg: "workspace dependency tree", time: "4 hours ago" },
                ].map((file) => (
                  <div key={file.name} className="sim-gh-file-row">
                    <span className="sim-gh-file-name">{file.name}</span>
                    <span className="sim-gh-file-msg">{file.msg}</span>
                    <span className="sim-gh-file-time">{file.time}</span>
                  </div>
                ))}
              </div>

              <div className="sim-gh-readme">
                <h2>README.md</h2>
                <div className="sim-gh-readme-body">
                  <p><strong>KAGE (影)</strong> — Developer-First Autonomous Browser built on Tauri 2.x, Chromium Embedded Framework (CEF), and React 18+ Liquid Glass UI.</p>
                  <p>Designed for engineers who want automated DevTools inspection, agentic test generation, and deep context awareness.</p>
                </div>
              </div>
            </div>
          ) : isVercel ? (
            <div className="sim-vercel">
              <div className="sim-vercel-header">
                <h1>Overview · Vercel Deployments</h1>
                <button className="sim-vercel-cta">+ New Project</button>
              </div>
              <div className="sim-vercel-cards">
                <div className="sim-vercel-card">
                  <div className="sim-vc-top">
                    <h3>kage-browser-preview</h3>
                    <span className="sim-vc-status">Production · Ready</span>
                  </div>
                  <p className="sim-vc-url">https://kage-browser.vercel.app</p>
                  <div className="sim-vc-footer">
                    <span>main (48a229c)</span>
                    <span>Deployed 4m ago by hardik</span>
                  </div>
                </div>
                <div className="sim-vercel-card">
                  <div className="sim-vc-top">
                    <h3>kage-docs</h3>
                    <span className="sim-vc-status">Preview · Ready</span>
                  </div>
                  <p className="sim-vc-url">https://kage-docs-git-feat.vercel.app</p>
                  <div className="sim-vc-footer">
                    <span>feat/design-tokens (71d9e8b)</span>
                    <span>Deployed 18m ago</span>
                  </div>
                </div>
              </div>
            </div>
          ) : isDocs ? (
            <div className="sim-docs">
              <aside className="sim-docs-sidebar">
                <h4>Documentation</h4>
                <ul>
                  <li className="active">Getting Started</li>
                  <li>Architecture Overview</li>
                  <li>CEF Integration</li>
                  <li>Tool Bus & Permissions</li>
                  <li>Context Engine API</li>
                </ul>
              </aside>
              <article className="sim-docs-article">
                <h1>Getting Started with KAGE</h1>
                <p className="lead">Learn how KAGE integrates the Chromium Embedded Framework with a Rust host process to deliver native 120 FPS performance and intelligent autonomous browsing.</p>
                <h3>System Requirements</h3>
                <ul>
                  <li>Rust 1.80+ (MSVC or GNU toolchain)</li>
                  <li>Node.js 20+ & npm 10+</li>
                  <li>Chromium Embedded Framework (CEF) v126+ runtime binaries</li>
                </ul>
              </article>
            </div>
          ) : (
            <div className="sim-generic">
              <div className="sim-generic-card">
                <h2>{url}</h2>
                <p>Simulated webview viewport loaded. Chromium Embedded Framework (CEF) OSR compositor target.</p>
                <div className="sim-generic-meta">
                  <span>Protocol: HTTP/2 TLS 1.3</span>
                  <span>Rendering Engine: Blink / V8</span>
                  <span>Security Sandbox: Level 2</span>
                </div>
              </div>
            </div>
          )}
        </div>
      </div>
    </div>
  );
};
