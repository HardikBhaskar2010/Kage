import React, { useState } from "react";
import "./NewTab.css";

const QUICK_LINKS = [
  { id: "github",  label: "GitHub",  url: "https://github.com",    char: "G" },
  { id: "vercel",  label: "Vercel",  url: "https://vercel.com",    char: "▲" },
  { id: "notion",  label: "Notion",  url: "https://notion.so",     char: "N" },
  { id: "youtube", label: "YouTube", url: "https://youtube.com",   char: "▶" },
  { id: "x",       label: "X",       url: "https://x.com",         char: "✕" },
  { id: "add",     label: "Add",     url: "#add",                  char: "+" },
];

interface NewTabProps {
  onNavigate: (url: string) => void;
}

export const NewTab: React.FC<NewTabProps> = ({ onNavigate }) => {
  const [query, setQuery] = useState("");

  const now = new Date();
  const timeStr = now.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
  const dateStr = now.toLocaleDateString([], { weekday: "short", day: "numeric", month: "short", year: "numeric" });

  const handleSearch = (e: React.FormEvent) => {
    e.preventDefault();
    if (!query.trim()) return;
    const isUrl = /^https?:\/\//.test(query) || /^[\w-]+\.[\w.]+/.test(query);
    onNavigate(isUrl ? query : `https://www.google.com/search?q=${encodeURIComponent(query)}`);
  };

  return (
    <div className="new-tab kage-bg" role="main">
      {/* Top-right: date & time */}
      <div className="new-tab__clock" aria-live="off">
        <time className="new-tab__time" dateTime={now.toISOString()}>{timeStr}</time>
        <time className="new-tab__date" dateTime={now.toDateString()}>{dateStr}</time>
      </div>

      {/* Center content */}
      <div className="new-tab__center">
        {/* Logo */}
        <div className="new-tab__logo" aria-label="KAGE Browser for Builders">
          <img src="/Logo.png" alt="KAGE" className="new-tab__logo-img" />
        </div>

        {/* Search bar */}
        <form className="new-tab__search-form" onSubmit={handleSearch} role="search" aria-label="Web search">
          <div className="new-tab__search-wrap">
            <svg className="new-tab__search-icon" width="18" height="18" viewBox="0 0 18 18" fill="none" aria-hidden="true">
              <circle cx="8" cy="8" r="5.5" stroke="currentColor" strokeWidth="1.5"/>
              <path d="M12 12l3 3" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round"/>
            </svg>
            <input
              id="new-tab-search"
              type="search"
              className="new-tab__search-input"
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              placeholder="Search the web or ask Kage AI…"
              aria-label="Search or enter a URL"
              autoFocus
            />
            <div className="new-tab__search-actions" aria-hidden="true">
              <span className="new-tab__search-divider" />
              <button type="button" className="new-tab__search-ai-btn" aria-label="Ask Kage AI">
                <svg width="16" height="16" viewBox="0 0 16 16" fill="none">
                  <path d="M8 2L9.5 6.5H14L10.5 9L12 13.5L8 11L4 13.5L5.5 9L2 6.5H6.5L8 2Z" stroke="currentColor" strokeWidth="1.2" strokeLinejoin="round"/>
                </svg>
              </button>
            </div>
          </div>
        </form>

        {/* Quick links */}
        <div className="new-tab__quick-links" role="list" aria-label="Quick links">
          {QUICK_LINKS.map((link) => (
            <div key={link.id} className="new-tab__quick-link-wrap" role="listitem">
              <button
                className="new-tab__quick-link"
                onClick={() => onNavigate(link.url)}
                aria-label={`Open ${link.label}`}
                id={`quick-link-${link.id}`}
              >
                <span className="new-tab__quick-link-icon" aria-hidden="true">
                  {link.char}
                </span>
              </button>
              <span className="new-tab__quick-link-label">{link.label}</span>
            </div>
          ))}
        </div>
      </div>

      {/* Bottom-left tagline */}
      <footer className="new-tab__footer" aria-label="KAGE tagline">
        <p className="new-tab__tagline-jp" lang="ja">つくる、もっと自由に。</p>
        <p className="new-tab__tagline-en">A FASTER WEB FOR A BRIGHTER TOMORROW.</p>
      </footer>
    </div>
  );
};
