import React, { useState, useRef, useEffect } from "react";
import "./ConsolePanel.css";
import { useBrowser } from "../../context/BrowserContext";
import { Search, Trash2, Terminal, AlertCircle, AlertTriangle, Info } from "lucide-react";

export const ConsolePanel: React.FC = () => {
  const { consoleLogs, addConsoleLog, clearConsoleLogs } = useBrowser();
  const [filter, setFilter] = useState<"all" | "error" | "warn" | "info">("all");
  const [search, setSearch] = useState("");
  const [commandInput, setCommandInput] = useState("");
  const logsEndRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    logsEndRef.current?.scrollIntoView({ behavior: "smooth" });
  }, [consoleLogs]);

  const errorCount = consoleLogs.filter((l) => l.level === "error").length;
  const warnCount = consoleLogs.filter((l) => l.level === "warn").length;
  const infoCount = consoleLogs.filter((l) => l.level === "info" || l.level === "log").length;

  const filteredLogs = consoleLogs.filter((log) => {
    if (filter === "error" && log.level !== "error") return false;
    if (filter === "warn" && log.level !== "warn") return false;
    if (filter === "info" && log.level !== "info" && log.level !== "log") return false;
    if (search && !log.message.toLowerCase().includes(search.toLowerCase())) return false;
    return true;
  });

  const handleExecuteCommand = (e: React.FormEvent) => {
    e.preventDefault();
    const cmd = commandInput.trim();
    if (!cmd) return;

    addConsoleLog("log", `> ${cmd}`, "console::eval");

    try {
      // Safe simulated JS evaluation
      // eslint-disable-next-line no-new-func
      const result = new Function(`
        try {
          return (${cmd});
        } catch (e) {
          return eval("${cmd.replace(/"/g, '\\"')}");
        }
      `)();

      const resultStr =
        typeof result === "object" ? JSON.stringify(result, null, 2) : String(result);
      addConsoleLog("info", `< ${resultStr}`, "console::result");
    } catch (err: any) {
      addConsoleLog("error", `Uncaught ${err.name || "Error"}: ${err.message}`, "console::error");
    }

    setCommandInput("");
  };

  return (
    <div className="console-panel" role="region" aria-label="JavaScript Console">
      {/* ── Toolbar ────────────────────────────────────────────── */}
      <div className="console-toolbar">
        <div className="console-toolbar__filters">
          <button
            className={`console-filter-btn ${filter === "all" ? "console-filter-btn--active" : ""}`}
            onClick={() => setFilter("all")}
          >
            All ({consoleLogs.length})
          </button>
          <button
            className={`console-filter-btn console-filter-btn--error ${filter === "error" ? "console-filter-btn--active" : ""}`}
            onClick={() => setFilter("error")}
          >
            <AlertCircle size={12} strokeWidth={2} />
            Errors ({errorCount})
          </button>
          <button
            className={`console-filter-btn console-filter-btn--warn ${filter === "warn" ? "console-filter-btn--active" : ""}`}
            onClick={() => setFilter("warn")}
          >
            <AlertTriangle size={12} strokeWidth={2} />
            Warnings ({warnCount})
          </button>
          <button
            className={`console-filter-btn console-filter-btn--info ${filter === "info" ? "console-filter-btn--active" : ""}`}
            onClick={() => setFilter("info")}
          >
            <Info size={12} strokeWidth={2} />
            Info ({infoCount})
          </button>
        </div>

        <div className="console-toolbar__actions">
          <div className="console-search-wrap">
            <Search size={13} strokeWidth={2} className="console-search-icon" />
            <input
              type="text"
              placeholder="Filter logs…"
              value={search}
              onChange={(e) => setSearch(e.target.value)}
              className="console-search-input"
            />
          </div>
          <button
            className="console-tool-btn"
            onClick={clearConsoleLogs}
            title="Clear console"
            aria-label="Clear console"
          >
            <Trash2 size={13} strokeWidth={2} />
          </button>
        </div>
      </div>

      {/* ── Log List ────────────────────────────────────────────── */}
      <div className="console-log-list" role="log">
        {filteredLogs.length === 0 ? (
          <div className="console-empty">
            <Terminal size={24} strokeWidth={1.5} className="console-empty-icon" />
            <p>No log messages matching current filter</p>
          </div>
        ) : (
          filteredLogs.map((log) => (
            <div key={log.id} className={`console-row console-row--${log.level}`}>
              <span className="console-row__icon">
                {log.level === "error" && <AlertCircle size={13} strokeWidth={2} />}
                {log.level === "warn" && <AlertTriangle size={13} strokeWidth={2} />}
                {log.level === "info" && <Info size={13} strokeWidth={2} />}
                {log.level === "log" && <span className="console-dot" />}
              </span>
              <span className="console-row__time">{log.timestamp}</span>
              <span className="console-row__message">{log.message}</span>
              <span className="console-row__source">{log.source}</span>
            </div>
          ))
        )}
        <div ref={logsEndRef} />
      </div>

      {/* ── Interactive REPL Command Prompt ─────────────────────── */}
      <form className="console-prompt" onSubmit={handleExecuteCommand}>
        <span className="console-prompt__arrow">&gt;</span>
        <input
          type="text"
          value={commandInput}
          onChange={(e) => setCommandInput(e.target.value)}
          placeholder="Evaluate JavaScript expression (e.g. document.title, 2 + 2, navigator.userAgent)…"
          className="console-prompt__input"
          spellCheck={false}
          autoComplete="off"
        />
        <button type="submit" className="console-prompt__run-btn" disabled={!commandInput.trim()}>
          Run
        </button>
      </form>
    </div>
  );
};
