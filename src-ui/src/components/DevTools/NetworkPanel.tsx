import React, { useState } from "react";
import "./NetworkPanel.css";
import { useBrowser } from "../../context/BrowserContext";
import type { NetworkRequest } from "../../context/BrowserContext";
import { Search, Trash2, X } from "lucide-react";
import { WaterfallTimeline } from "../ui";

export const NetworkPanel: React.FC = () => {
  const { networkRequests, clearNetworkRequests } = useBrowser();
  const [selectedReq, setSelectedReq] = useState<NetworkRequest | null>(null);
  const [typeFilter, setTypeFilter] = useState<string>("all");
  const [search, setSearch] = useState("");

  const filteredRequests = networkRequests.filter((req) => {
    if (typeFilter !== "all" && req.type !== typeFilter) return false;
    if (search && !req.url.toLowerCase().includes(search.toLowerCase())) return false;
    return true;
  });

  const getStatusClass = (status: number) => {
    if (status >= 200 && status < 300) return "net-status--2xx";
    if (status >= 300 && status < 400) return "net-status--3xx";
    if (status >= 400 && status < 500) return "net-status--4xx";
    return "net-status--5xx";
  };

  return (
    <div className="network-panel" role="region" aria-label="Network Monitor">
      {/* ── Toolbar ────────────────────────────────────────────── */}
      <div className="net-toolbar">
        <div className="net-toolbar__filters">
          {["all", "fetch", "script", "stylesheet", "image"].map((t) => (
            <button
              key={t}
              className={`net-filter-btn ${typeFilter === t ? "net-filter-btn--active" : ""}`}
              onClick={() => setTypeFilter(t)}
            >
              {t.toUpperCase()}
            </button>
          ))}
        </div>

        <div className="net-toolbar__actions">
          <div className="net-search-wrap">
            <Search size={13} strokeWidth={2} className="net-search-icon" />
            <input
              type="text"
              placeholder="Filter URL…"
              value={search}
              onChange={(e) => setSearch(e.target.value)}
              className="net-search-input"
            />
          </div>
          <button
            className="net-tool-btn"
            onClick={clearNetworkRequests}
            title="Clear network log"
            aria-label="Clear network log"
          >
            <Trash2 size={13} strokeWidth={2} />
          </button>
        </div>
      </div>

      {/* ── Requests Table & Split View ─────────────────────────── */}
      <div className="net-body">
        <div className={`net-table-wrap ${selectedReq ? "net-table-wrap--split" : ""}`}>
          <table className="net-table">
            <thead>
              <tr>
                <th style={{ width: "70px" }}>Status</th>
                <th style={{ width: "65px" }}>Method</th>
                <th>Name / URL</th>
                <th style={{ width: "70px" }}>Type</th>
                <th style={{ width: "75px" }}>Size</th>
                <th style={{ width: "120px" }}>Waterfall</th>
              </tr>
            </thead>
            <tbody>
              {filteredRequests.map((req) => {
                const urlParts = req.url.split("/");
                const name = urlParts[urlParts.length - 1] || req.url;
                const isSelected = selectedReq?.id === req.id;

                return (
                  <tr
                    key={req.id}
                    className={`net-row ${isSelected ? "net-row--selected" : ""}`}
                    onClick={() => setSelectedReq(isSelected ? null : req)}
                  >
                    <td>
                      <span className={`net-status-badge ${getStatusClass(req.status)}`}>
                        {req.status}
                      </span>
                    </td>
                    <td>
                      <span className="net-method">{req.method}</span>
                    </td>
                    <td className="net-name" title={req.url}>
                      <span className="net-name-primary">{name}</span>
                      <span className="net-domain">{req.url.replace(/^https?:\/\//, "").split("/")[0]}</span>
                    </td>
                    <td className="net-type">{req.type}</td>
                    <td className="net-size">{req.size}</td>
                    <td>
                      <WaterfallTimeline totalMs={req.time} maxScaleMs={400} />
                    </td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        </div>

        {/* ── Request Detail Drawer ─────────────────────────────── */}
        {selectedReq && (
          <aside className="net-detail" aria-label="Request details">
            <div className="net-detail__header">
              <span className="net-detail__title">Request Details</span>
              <button
                className="net-detail__close"
                onClick={() => setSelectedReq(null)}
                aria-label="Close details"
              >
                <X size={14} strokeWidth={2} />
              </button>
            </div>

            <div className="net-detail__content">
              <div className="net-detail__section">
                <h4>General</h4>
                <div className="net-kv">
                  <span className="net-k">Request URL:</span>
                  <span className="net-v">{selectedReq.url}</span>
                </div>
                <div className="net-kv">
                  <span className="net-k">Request Method:</span>
                  <span className="net-v">{selectedReq.method}</span>
                </div>
                <div className="net-kv">
                  <span className="net-k">Status Code:</span>
                  <span className="net-v">{selectedReq.status} {selectedReq.statusText}</span>
                </div>
              </div>

              <div className="net-detail__section">
                <h4>Response Headers</h4>
                {Object.entries(selectedReq.headers).map(([k, v]) => (
                  <div key={k} className="net-kv">
                    <span className="net-k">{k}:</span>
                    <span className="net-v">{v}</span>
                  </div>
                ))}
              </div>

              {selectedReq.responseSnippet && (
                <div className="net-detail__section">
                  <h4>Response Preview</h4>
                  <pre className="net-preview">{selectedReq.responseSnippet}</pre>
                </div>
              )}
            </div>
          </aside>
        )}
      </div>
    </div>
  );
};
