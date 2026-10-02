import React, { useState, useRef, useEffect, useCallback } from "react";
import "./ConsolePanel.css";
import { useBrowser } from "../../context/BrowserContext";
import { evalJs } from "../../ipc/client";
import {
  Search,
  Trash2,
  Terminal,
  AlertCircle,
  AlertTriangle,
  Info,
  Sparkles,
  Copy,
  Check,
  CornerDownLeft,
} from "lucide-react";

export const ConsolePanel: React.FC = () => {
  const {
    activeTabId,
    consoleLogs,
    addConsoleLog,
    clearConsoleLogs,
    setActiveTool,
    setAiOpen,
  } = useBrowser();

  const [filter, setFilter] = useState<"all" | "error" | "warn" | "info">("all");
  const [search, setSearch] = useState("");
  const [commandInput, setCommandInput] = useState("");
  const [commandHistory, setCommandHistory] = useState<string[]>([]);
  const [historyIndex, setHistoryIndex] = useState<number>(-1);
  const [copiedLogId, setCopiedLogId] = useState<string | null>(null);
  const [isEvaluating, setIsEvaluating] = useState(false);

  const logsEndRef = useRef<HTMLDivElement>(null);
  const inputRef = useRef<HTMLInputElement>(null);

  // Auto-scroll on new logs
  useEffect(() => {
    logsEndRef.current?.scrollIntoView({ behavior: "smooth" });
  }, [consoleLogs]);

  // Keyboard shortcut Ctrl+L / Cmd+K to clear console
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if ((e.ctrlKey && e.key === "l") || (e.metaKey && e.key === "k")) {
        e.preventDefault();
        clearConsoleLogs();
      }
    };
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [clearConsoleLogs]);

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

  const handleExecuteCommand = async (e?: React.FormEvent) => {
    if (e) e.preventDefault();
    const cmd = commandInput.trim();
    if (!cmd || isEvaluating) return;

    // Push into history and reset index
    setCommandHistory((prev) => [...prev, cmd]);
    setHistoryIndex(-1);

    // Echo input into console log stream
    addConsoleLog("log", `> ${cmd}`, "console::eval");
    setCommandInput("");
    setIsEvaluating(true);

    try {
      // Governed execution through host ToolBus -> devtools.runtime.evaluate (INV-02)
      const res: any = await evalJs(cmd, activeTabId);

      let formattedOutput = "";
      if (res && typeof res === "object") {
        if ("result" in res) {
          const val = res.result?.value;
          const desc = res.result?.description;
          if (val !== undefined) {
            formattedOutput = typeof val === "object" ? JSON.stringify(val, null, 2) : String(val);
          } else if (desc) {
            formattedOutput = desc;
          } else {
            formattedOutput = String(res.result?.type || "undefined");
          }
        } else if ("exceptionDetails" in res) {
          const exc = res.exceptionDetails;
          const msg = exc.exception?.description || exc.text || "Runtime evaluation error";
          addConsoleLog("error", `Uncaught ${msg}`, "console::error");
          setIsEvaluating(false);
          return;
        } else {
          formattedOutput = JSON.stringify(res, null, 2);
        }
      } else if (res === undefined) {
        formattedOutput = "undefined";
      } else {
        formattedOutput = String(res);
      }

      addConsoleLog("info", `< ${formattedOutput}`, "console::result");
    } catch (err: any) {
      const errMsg = err?.message || String(err) || "Evaluation failed";
      addConsoleLog("error", `Uncaught ${errMsg}`, "console::error");
    } finally {
      setIsEvaluating(false);
    }
  };

  const handleKeyDown = (e: React.KeyboardEvent<HTMLInputElement>) => {
    if (e.key === "ArrowUp") {
      e.preventDefault();
      if (commandHistory.length === 0) return;
      const nextIdx = historyIndex === -1 ? commandHistory.length - 1 : Math.max(0, historyIndex - 1);
      setHistoryIndex(nextIdx);
      setCommandInput(commandHistory[nextIdx]);
    } else if (e.key === "ArrowDown") {
      e.preventDefault();
      if (commandHistory.length === 0 || historyIndex === -1) return;
      const nextIdx = historyIndex + 1;
      if (nextIdx >= commandHistory.length) {
        setHistoryIndex(-1);
        setCommandInput("");
      } else {
        setHistoryIndex(nextIdx);
        setCommandInput(commandHistory[nextIdx]);
      }
    }
  };

  const copyToClipboard = useCallback((text: string, logId: string) => {
    navigator.clipboard.writeText(text);
    setCopiedLogId(logId);
    setTimeout(() => setCopiedLogId(null), 1500);
  }, []);

  const explainWithAi = useCallback((errorMessage: string) => {
    setActiveTool("ai");
    setAiOpen(true);
    // Dispatched via BrowserContext for AI Copilot preloaded context
    window.dispatchEvent(
      new CustomEvent("kage:ai:prompt", {
        detail: {
          prompt: `Please explain and diagnose this browser console error:\n\n\`\`\`\n${errorMessage}\n\`\`\``,
        },
      })
    );
  }, [setActiveTool, setAiOpen]);

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
              placeholder="Filter logs (Ctrl+F)…"
              value={search}
              onChange={(e) => setSearch(e.target.value)}
              className="console-search-input"
            />
          </div>
          <button
            className="console-tool-btn"
            onClick={clearConsoleLogs}
            title="Clear console (Ctrl+L)"
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

              {/* Action buttons on log row */}
              <div className="console-row__actions">
                {log.level === "error" && (
                  <button
                    className="console-explain-btn"
                    onClick={() => explainWithAi(log.message)}
                    title="Explain with AI Copilot"
                  >
                    <Sparkles size={11} strokeWidth={2} />
                    <span>Explain</span>
                  </button>
                )}
                <button
                  className="console-copy-btn"
                  onClick={() => copyToClipboard(log.message, log.id)}
                  title="Copy log text"
                >
                  {copiedLogId === log.id ? (
                    <Check size={11} strokeWidth={2} />
                  ) : (
                    <Copy size={11} strokeWidth={2} />
                  )}
                </button>
              </div>

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
          ref={inputRef}
          type="text"
          value={commandInput}
          onChange={(e) => setCommandInput(e.target.value)}
          onKeyDown={handleKeyDown}
          placeholder="Evaluate JavaScript expression via ToolBus (e.g. document.title, location.href, 2 + 2)…"
          className="console-prompt__input"
          spellCheck={false}
          autoComplete="off"
          disabled={isEvaluating}
        />
        <button
          type="submit"
          className="console-prompt__run-btn"
          disabled={!commandInput.trim() || isEvaluating}
          title="Execute (Enter)"
        >
          {isEvaluating ? (
            <span className="console-spinner" />
          ) : (
            <CornerDownLeft size={13} strokeWidth={2} />
          )}
        </button>
      </form>
    </div>
  );
};

