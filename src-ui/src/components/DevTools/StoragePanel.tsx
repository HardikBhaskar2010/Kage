import React, { useState, useEffect, useCallback } from "react";
import "./StoragePanel.css";
import { Plus, Trash2, Search, RefreshCw } from "lucide-react";
import { useBrowser } from "../../context/BrowserContext";
import { isTauriEnvironment } from "../../adapters/BrowserAdapter";
import { getCookies, getLocalStorage, getSessionStorage, evalJs } from "../../ipc/client";

interface StorageItem {
  key: string;
  value: string;
}

export const StoragePanel: React.FC = () => {
  const { activeTabId } = useBrowser();
  const [activeTab, setActiveTab] = useState<"local" | "session" | "cookies">("local");
  const [search, setSearch] = useState("");
  const [isLoading, setIsLoading] = useState(false);

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

  const fetchStorage = useCallback(async () => {
    if (!isTauriEnvironment() || !activeTabId) return;
    setIsLoading(true);
    try {
      if (activeTab === "cookies") {
        const res: any = await getCookies(activeTabId);
        if (res && Array.isArray(res.cookies)) {
          const mapped: StorageItem[] = res.cookies.map((c: any) => ({
            key: c.name || "cookie",
            value: `${c.value} [${c.domain || ""}${c.path || ""}]`,
          }));
          setItems(mapped);
        } else {
          setItems([]);
        }
      } else if (activeTab === "local") {
        const res: any = await getLocalStorage(activeTabId);
        if (res && res.items && typeof res.items === "object") {
          const mapped: StorageItem[] = Object.entries(res.items).map(([k, v]) => ({
            key: k,
            value: String(v),
          }));
          setItems(mapped);
        } else {
          setItems([]);
        }
      } else if (activeTab === "session") {
        const res: any = await getSessionStorage(activeTabId);
        if (res && res.items && typeof res.items === "object") {
          const mapped: StorageItem[] = Object.entries(res.items).map(([k, v]) => ({
            key: k,
            value: String(v),
          }));
          setItems(mapped);
        } else {
          setItems([]);
        }
      }
    } catch (err) {
      console.warn("Storage fetch error:", err);
    } finally {
      setIsLoading(false);
    }
  }, [activeTab, activeTabId]);

  useEffect(() => {
    if (isTauriEnvironment() && activeTabId) {
      void fetchStorage();
    }
  }, [fetchStorage, activeTabId]);

  const handleAdd = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!newKey.trim()) return;
    const k = newKey.trim();
    const v = newVal.trim();

    if (isTauriEnvironment() && activeTabId) {
      try {
        if (activeTab === "local") {
          await evalJs(`localStorage.setItem(${JSON.stringify(k)}, ${JSON.stringify(v)})`, activeTabId);
        } else if (activeTab === "session") {
          await evalJs(`sessionStorage.setItem(${JSON.stringify(k)}, ${JSON.stringify(v)})`, activeTabId);
        } else if (activeTab === "cookies") {
          await evalJs(`document.cookie = ${JSON.stringify(`${encodeURIComponent(k)}=${encodeURIComponent(v)}; path=/`)}`, activeTabId);
        }
        await fetchStorage();
      } catch (err) {
        console.warn("Failed to set storage item:", err);
      }
    } else {
      setItems((prev) => [...prev, { key: k, value: v }]);
    }

    setNewKey("");
    setNewVal("");
    setShowAdd(false);
  };

  const handleDelete = async (keyToDelete: string) => {
    if (isTauriEnvironment() && activeTabId) {
      try {
        if (activeTab === "local") {
          await evalJs(`localStorage.removeItem(${JSON.stringify(keyToDelete)})`, activeTabId);
        } else if (activeTab === "session") {
          await evalJs(`sessionStorage.removeItem(${JSON.stringify(keyToDelete)})`, activeTabId);
        } else if (activeTab === "cookies") {
          await evalJs(`document.cookie = ${JSON.stringify(`${encodeURIComponent(keyToDelete)}=; Max-Age=0; path=/`)}`, activeTabId);
        }
        await fetchStorage();
      } catch (err) {
        console.warn("Failed to delete storage item:", err);
      }
    } else {
      setItems((prev) => prev.filter((i) => i.key !== keyToDelete));
    }
  };

  const handleClearAll = async () => {
    if (isTauriEnvironment() && activeTabId) {
      try {
        if (activeTab === "local") {
          await evalJs("localStorage.clear()", activeTabId);
        } else if (activeTab === "session") {
          await evalJs("sessionStorage.clear()", activeTabId);
        }
        await fetchStorage();
      } catch (err) {
        console.warn("Failed to clear storage:", err);
      }
    } else {
      setItems([]);
    }
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
            onClick={fetchStorage}
            title="Refresh storage"
            aria-label="Refresh storage"
          >
            <RefreshCw size={13} strokeWidth={2} className={isLoading ? "storage-refresh--spinning" : ""} />
          </button>
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
            onClick={handleClearAll}
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
            {filteredItems.length === 0 ? (
              <tr className="storage-row storage-row--empty">
                <td colSpan={3} style={{ textAlign: "center", color: "rgba(249, 219, 189, 0.4)", padding: "24px 0" }}>
                  {isLoading ? "Loading storage items..." : "No items found in storage"}
                </td>
              </tr>
            ) : (
              filteredItems.map((item) => (
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
              ))
            )}
          </tbody>
        </table>
      </div>
    </div>
  );
};
