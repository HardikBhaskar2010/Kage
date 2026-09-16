import React, { useState, useEffect } from "react";
import "./MicroInspectOverlay.css";
import { useBrowser } from "../../context/BrowserContext";
import { Sparkles, X, Code, Copy, Check } from "lucide-react";

interface ElementTarget {
  tag: string;
  className: string;
  rect: DOMRect;
  computedCss: Record<string, string>;
}

export const MicroInspectOverlay: React.FC = () => {
  const { inspectMode, setInspectMode, setAiOpen, setActiveTool } = useBrowser();
  const [hoveredEl, setHoveredEl] = useState<ElementTarget | null>(null);
  const [pinnedEl, setPinnedEl] = useState<ElementTarget | null>(null);
  const [copied, setCopied] = useState(false);

  useEffect(() => {
    if (!inspectMode) {
      setHoveredEl(null);
      setPinnedEl(null);
      return;
    }

    const handleMouseMove = (e: MouseEvent) => {
      if (pinnedEl) return;
      const target = document.elementFromPoint(e.clientX, e.clientY) as HTMLElement | null;
      if (!target || target.closest(".micro-inspect-ignore") || target.closest(".icon-sidebar") || target.closest(".omnibox") || target.closest(".app__titlebar") || target.closest(".ai-sidebar") || target.closest(".devtools-drawer")) {
        setHoveredEl(null);
        return;
      }

      const rect = target.getBoundingClientRect();
      const style = window.getComputedStyle(target);
      setHoveredEl({
        tag: target.tagName.toLowerCase(),
        className: target.className && typeof target.className === "string" ? target.className.split(" ")[0] : "",
        rect,
        computedCss: {
          display: style.display,
          color: style.color,
          fontSize: style.fontSize,
          padding: style.padding,
          borderRadius: style.borderRadius,
        },
      });
    };

    const handleClick = (e: MouseEvent) => {
      const target = document.elementFromPoint(e.clientX, e.clientY) as HTMLElement | null;
      if (!target || target.closest(".micro-inspect-ignore") || target.closest(".icon-sidebar") || target.closest(".omnibox") || target.closest(".app__titlebar")) {
        return;
      }
      e.preventDefault();
      e.stopPropagation();

      if (hoveredEl) {
        setPinnedEl(hoveredEl);
      }
    };

    window.addEventListener("mousemove", handleMouseMove);
    window.addEventListener("click", handleClick, true);

    return () => {
      window.removeEventListener("mousemove", handleMouseMove);
      window.removeEventListener("click", handleClick, true);
    };
  }, [inspectMode, pinnedEl, hoveredEl]);

  if (!inspectMode) return null;

  const current = pinnedEl || hoveredEl;

  const handleSendToAi = () => {
    if (!current) return;
    setAiOpen(true);
    setActiveTool("ai");
    setInspectMode(false);
  };

  const handleCopySelector = () => {
    if (!current) return;
    const selector = `${current.tag}${current.className ? `.${current.className}` : ""}`;
    navigator.clipboard.writeText(selector);
    setCopied(true);
    setTimeout(() => setCopied(false), 1200);
  };

  return (
    <div className="micro-inspect-root micro-inspect-ignore" aria-label="Micro Inspect Overlay">
      {/* ── Active Target Bounding Box ──────────────────────────── */}
      {current && (
        <div
          className="micro-inspect-box"
          style={{
            top: `${current.rect.top}px`,
            left: `${current.rect.left}px`,
            width: `${current.rect.width}px`,
            height: `${current.rect.height}px`,
          }}
        >
          <div className="micro-inspect-badge">
            <span className="mib-tag">{current.tag}</span>
            {current.className && <span className="mib-class">.{current.className}</span>}
            <span className="mib-dims">{Math.round(current.rect.width)} × {Math.round(current.rect.height)}</span>
          </div>
        </div>
      )}

      {/* ── Pinned Inspector Card ───────────────────────────────── */}
      {pinnedEl && (
        <div
          className="micro-inspect-card glass-panel"
          style={{
            top: `${Math.min(window.innerHeight - 240, pinnedEl.rect.bottom + 8)}px`,
            left: `${Math.min(window.innerWidth - 320, Math.max(70, pinnedEl.rect.left))}px`,
          }}
        >
          <div className="mic-header">
            <div className="mic-title">
              <Code size={14} strokeWidth={2} />
              <span>&lt;{pinnedEl.tag}{pinnedEl.className ? `.${pinnedEl.className}` : ""}&gt;</span>
            </div>
            <button className="mic-close-btn" onClick={() => setPinnedEl(null)}>
              <X size={13} strokeWidth={2} />
            </button>
          </div>

          <div className="mic-body">
            <div className="mic-kv">
              <span>Dimensions:</span>
              <strong>{Math.round(pinnedEl.rect.width)} × {Math.round(pinnedEl.rect.height)} px</strong>
            </div>
            <div className="mic-kv">
              <span>Display:</span>
              <strong>{pinnedEl.computedCss.display}</strong>
            </div>
            <div className="mic-kv">
              <span>Font Size:</span>
              <strong>{pinnedEl.computedCss.fontSize}</strong>
            </div>
            <div className="mic-kv">
              <span>Border Radius:</span>
              <strong>{pinnedEl.computedCss.borderRadius}</strong>
            </div>
          </div>

          <div className="mic-actions">
            <button className="mic-btn mic-btn--ai" onClick={handleSendToAi}>
              <Sparkles size={13} strokeWidth={2} />
              Send to Kage AI
            </button>
            <button className="mic-btn mic-btn--copy" onClick={handleCopySelector}>
              {copied ? <Check size={13} strokeWidth={2} /> : <Copy size={13} strokeWidth={2} />}
              {copied ? "Copied" : "Copy Selector"}
            </button>
          </div>
        </div>
      )}

      {/* ── Exit Floating Bar ───────────────────────────────────── */}
      <div className="micro-inspect-exit-bar">
        <span>Micro-Inspect Mode Active · Click any element to pin</span>
        <button className="mib-exit-btn" onClick={() => setInspectMode(false)}>
          Exit (Esc)
        </button>
      </div>
    </div>
  );
};
