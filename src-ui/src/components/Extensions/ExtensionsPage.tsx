import React, { useState } from "react";
import "./ExtensionsPage.css";
import { Blocks, Plus, ShieldCheck } from "lucide-react";

interface ExtensionItem {
  id: string;
  name: string;
  version: string;
  author: string;
  description: string;
  enabled: boolean;
  capabilities: string[];
}

const INITIAL_EXTENSIONS: ExtensionItem[] = [
  {
    id: "react-devtools",
    name: "React Developer Tools",
    version: "v5.2.0",
    author: "Meta / Kage Wasm",
    description: "Inspect the React component tree, props, state, and hooks in real time.",
    enabled: true,
    capabilities: ["cdp:dom:inspect", "cdp:runtime:eval"],
  },
  {
    id: "vite-refresher",
    name: "Vite Fast Refresher",
    version: "v2.1.0",
    author: "Vite Core",
    description: "High-speed Hot Module Replacement bridge with instant sub-10ms state updates.",
    enabled: true,
    capabilities: ["cdp:network:ws", "tool:patch_state"],
  },
  {
    id: "graphql-inspector",
    name: "GraphQL Network Inspector",
    version: "v1.4.2",
    author: "GraphQL Guild",
    description: "Deep inspection and schema auto-completion for GraphQL queries and mutations.",
    enabled: false,
    capabilities: ["cdp:network:monitor"],
  },
  {
    id: "wasm-profiler",
    name: "Wasm Performance Profiler",
    version: "v0.9.1",
    author: "Bytecode Alliance",
    description: "Low-overhead CPU sampling and memory allocation tracking for WebAssembly binaries.",
    enabled: true,
    capabilities: ["wasm:memory", "sys:timer"],
  },
];

export const ExtensionsPage: React.FC = () => {
  const [extensions, setExtensions] = useState<ExtensionItem[]>(INITIAL_EXTENSIONS);
  const [installedCount, setInstalledCount] = useState(4);

  const toggleExtension = (id: string) => {
    setExtensions((prev) =>
      prev.map((ext) => (ext.id === id ? { ...ext, enabled: !ext.enabled } : ext))
    );
  };

  const handleInstallDummy = () => {
    const newExt: ExtensionItem = {
      id: `custom-ext-${Date.now()}`,
      name: `Custom Dev Plugin #${installedCount + 1}`,
      version: "v1.0.0",
      author: "Local Builder",
      description: "User-installed WebAssembly extension running in Wasmtime sandbox.",
      enabled: true,
      capabilities: ["cdp:console:read", "tool:execute"],
    };
    setExtensions((prev) => [newExt, ...prev]);
    setInstalledCount((c) => c + 1);
  };

  return (
    <div className="extensions-page" role="main" aria-label="KAGE Extensions & Plugins">
      <div className="ext-container">
        {/* Header */}
        <div className="ext-header">
          <div>
            <h1 className="ext-title">Extensions & Plugins</h1>
            <p className="ext-subtitle">
              Secure WebAssembly (Wasmtime) sandboxed plugins extending KAGE browser capabilities.
            </p>
          </div>
          <button className="ext-install-btn" onClick={handleInstallDummy}>
            <Plus size={15} strokeWidth={2.2} />
            Install Plugin (.wasm)
          </button>
        </div>

        {/* Extensions List */}
        <div className="ext-grid">
          {extensions.map((ext) => (
            <div key={ext.id} className={`ext-card ${ext.enabled ? "ext-card--enabled" : ""}`}>
              <div className="ext-card__top">
                <div className="ext-card__icon">
                  <Blocks size={20} strokeWidth={1.8} />
                </div>
                <div className="ext-card__meta">
                  <div className="ext-card__title-row">
                    <h3 className="ext-card__name">{ext.name}</h3>
                    <span className="ext-card__version">{ext.version}</span>
                  </div>
                  <span className="ext-card__author">{ext.author}</span>
                </div>

                {/* Spring Toggle Switch */}
                <button
                  type="button"
                  className={`ext-switch ${ext.enabled ? "ext-switch--on" : ""}`}
                  onClick={() => toggleExtension(ext.id)}
                  role="switch"
                  aria-checked={ext.enabled}
                  aria-label={`Toggle ${ext.name}`}
                >
                  <span className="ext-switch__thumb" />
                </button>
              </div>

              <p className="ext-card__desc">{ext.description}</p>

              <div className="ext-card__footer">
                <div className="ext-capabilities">
                  <ShieldCheck size={12} strokeWidth={2} className="ext-cap-icon" />
                  {ext.capabilities.map((cap) => (
                    <span key={cap} className="ext-cap-chip">{cap}</span>
                  ))}
                </div>
              </div>
            </div>
          ))}
        </div>
      </div>
    </div>
  );
};
