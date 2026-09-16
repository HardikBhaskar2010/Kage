import React, { useState } from "react";
import "./StoragePanel.css";
import { Plus, Trash2, Search } from "lucide-react";

interface StorageItem {
  key: string;
  value: string;
}

export const StoragePanel: React.FC = () => {
  const [activeTab, setActiveTab] = useState<"local" | "session" | "cookies">("local");
  const [search, setSearch] = useState("");

  const [items, setItems] = useState<StorageItem[]>([
    { key: "kage_theme", value: '"liquid-glass-peach"' },
    { key: "kage_active_workspace", value: '"dev-main"' },
    { key: "kage_ai_model", value: '"gpt-4o"' },
    { key: "kage_audit_enabled", value: "true" },
    { key: "auth_token_sample", value: '"eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9..."' },
  ]);

  const [newKey, setNewKey] = useState("");
  const [newVal, setNewVal] = useState("");
  const [showAdd, setShowAdd] = useState(false);

  const handleAdd = (e: React.FormEvent) => {
    e.preventDefault();
    if (!newKey.trim()) return;
    setItems((prev) => [...prev, { key: newKey.trim(), value: newVal.trim() }]);
    setNewKey("");
    setNewVal("");
    setShowAdd(false);
  };

  const handleDelete = (keyToDelete: string) => {
    setItems((prev) => prev.filter((i) => i.key !== keyToDelete));
  };

  const filteredItems = items.filter(
    (i) =>
      i.key.toLowerCase().includes(search.toLowerCase()) ||
      i.value.toLowerCase().includes(search.toLowerCase())
  );

  return (
    <div className="storage-panel" role="region" aria-label="Web Storage Inspector">
      {/* ── Toolbar ────────────────────────────────────────────── */}
      <div className="storage-toolbar">
        <div className="storage-toolbar__tabs">
          {(["local", "session", "cookies"] as const).map((tab) => (
            <button
              key={tab}
              className={`storage-tab-btn ${activeTab === tab ? "storage-tab-btn--active" : ""}`}
              onClick={() => setActiveTab(tab)}
            >
              {tab === "local" ? "LocalStorage" : tab === "session" ? "SessionStorage" : "Cookies"}
            </button>
          ))}
        </div>

        <div className="storage-toolbar__actions">
          <div className="storage-search-wrap">
            <Search size={13} strokeWidth={2} className="storage-search-icon" />
            <input
              type="text"
              placeholder="Filter key/value…"
              value={search}
              onChange={(e) => setSearch(e.target.value)}
              className="storage-search-input"
            />
          </div>
          <button
            className="storage-tool-btn"
            onClick={() => setShowAdd(true)}
            title="Add Item"
            aria-label="Add Item"
          >
            <Plus size={14} strokeWidth={2} />
          </button>
          <button
            className="storage-tool-btn"
            onClick={() => setItems([])}
            title="Clear all"
            aria-label="Clear all"
          >
            <Trash2 size={13} strokeWidth={2} />
          </button>
        </div>
      </div>

      {/* ── Add Modal Drawer ───────────────────────────────────── */}
      {showAdd && (
        <form className="storage-add-form" onSubmit={handleAdd}>
          <input
            type="text"
            placeholder="Key"
            value={newKey}
            onChange={(e) => setNewKey(e.target.value)}
            className="storage-add-input"
            autoFocus
          />
          <input
            type="text"
            placeholder="Value"
            value={newVal}
            onChange={(e) => setNewVal(e.target.value)}
            className="storage-add-input"
          />
          <div className="storage-add-btns">
            <button type="submit" className="storage-add-submit" disabled={!newKey.trim()}>
              Save
            </button>
            <button type="button" className="storage-add-cancel" onClick={() => setShowAdd(false)}>
              Cancel
            </button>
          </div>
        </form>
      )}

      {/* ── Storage Key-Value Table ─────────────────────────────── */}
      <div className="storage-table-wrap">
        <table className="storage-table">
          <thead>
            <tr>
              <th style={{ width: "35%" }}>Key</th>
              <th>Value</th>
              <th style={{ width: "40px" }} />
            </tr>
          </thead>
          <tbody>
            {filteredItems.map((item) => (
              <tr key={item.key} className="storage-row">
                <td className="storage-key">{item.key}</td>
                <td className="storage-val">{item.value}</td>
                <td>
                  <button
                    className="storage-del-btn"
                    onClick={() => handleDelete(item.key)}
                    title="Delete item"
                    aria-label={`Delete ${item.key}`}
                  >
                    <Trash2 size={12} strokeWidth={1.8} />
                  </button>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </div>
  );
};
