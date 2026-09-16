import React, { useState, useRef, useEffect } from "react";
import "./NewTab.css";
import {
  Search,
  Sparkles,
  Mic,
  Plus,
  Sun,
} from "lucide-react";

// Crisp SVG Brand Icons
const GithubIcon = () => (
  <svg width="20" height="20" viewBox="0 0 24 24" fill="currentColor">
    <path fillRule="evenodd" clipRule="evenodd" d="M12 2C6.477 2 2 6.484 2 12.017c0 4.425 2.865 8.18 6.839 9.504.5.092.682-.217.682-.483 0-.237-.008-.868-.013-1.703-2.782.605-3.369-1.343-3.369-1.343-.454-1.158-1.11-1.466-1.11-1.466-.908-.62.069-.608.069-.608 1.003.07 1.53 1.032 1.53 1.032.892 1.53 2.341 1.088 2.91.832.092-.647.35-1.088.636-1.338-2.22-.253-4.555-1.113-4.555-4.951 0-1.093.39-1.988 1.029-2.688-.103-.253-.446-1.272.098-2.65 0 0 .84-.27 2.75 1.026A9.564 9.564 0 0112 6.844c.85.004 1.705.115 2.504.337 1.909-1.296 2.747-1.027 2.747-1.027.546 1.379.202 2.398.1 2.651.64.7 1.028 1.595 1.028 2.688 0 3.848-2.339 4.695-4.566 4.943.359.309.678.92.678 1.855 0 1.338-.012 2.419-.012 2.747 0 .268.18.58.688.482A10.019 10.019 0 0022 12.017C22 6.484 17.522 2 12 2z"/>
  </svg>
);

const VercelIcon = () => (
  <svg width="17" height="17" viewBox="0 0 24 24" fill="currentColor">
    <path d="M12 1L24 22H0L12 1z"/>
  </svg>
);

const NotionIcon = () => (
  <svg width="18" height="18" viewBox="0 0 24 24" fill="currentColor">
    <path d="M4.459 4.208c.746.606 1.026.56 2.428.466l11.459-.7c.373 0 .466-.186.373-.466L17.799 2.25c-.28-.373-.746-.56-1.306-.56L3.993 2.81c-.466 0-.653.28-.466.56l.932.838zm.746 3.078c-.466 0-.653.28-.653.653v12.404c0 .466.28.746.746.746l13.71.746c.466 0 .746-.28.746-.746V7.939c0-.466-.28-.746-.746-.746L5.205 7.286zm3.358 2.705h2.238l3.731 5.69v-5.69h2.052v7.462h-2.052l-3.918-5.97v5.97H8.563V9.991z"/>
  </svg>
);

const YoutubeIcon = () => (
  <svg width="20" height="20" viewBox="0 0 24 24" fill="currentColor">
    <path d="M23.498 6.186a3.016 3.016 0 0 0-2.122-2.136C19.505 3.545 12 3.545 12 3.545s-7.505 0-9.377.505A3.017 3.017 0 0 0 .502 6.186C0 8.07 0 12 0 12s0 3.93.502 5.814a3.016 3.016 0 0 0 2.122 2.136c1.871.505 9.376.505 9.376.505s7.505 0 9.377-.505a3.015 3.015 0 0 0 2.122-2.136C24 15.93 24 12 24 12s0-3.93-.502-5.814zM9.545 15.568V8.432L15.818 12l-6.273 3.568z"/>
  </svg>
);

const XIcon = () => (
  <svg width="17" height="17" viewBox="0 0 24 24" fill="currentColor">
    <path d="M18.244 2.25h3.308l-7.227 8.26 8.502 11.24H16.17l-5.214-6.817L4.99 21.75H1.68l7.73-8.835L1.254 2.25H8.08l4.713 6.231zm-1.161 17.52h1.833L7.084 4.126H5.117z"/>
  </svg>
);

interface QuickLinkItem {
  id: string;
  label: string;
  category: string;
  url: string;
  icon: React.ReactNode;
}

const QUICK_LINKS: QuickLinkItem[] = [
  {
    id: "github",
    label: "GitHub",
    category: "Repository",
    url: "https://github.com",
    icon: <GithubIcon />,
  },
  {
    id: "vercel",
    label: "Vercel",
    category: "Deployments",
    url: "https://vercel.com",
    icon: <VercelIcon />,
  },
  {
    id: "notion",
    label: "Notion",
    category: "Workspace",
    url: "https://notion.so",
    icon: <NotionIcon />,
  },
  {
    id: "youtube",
    label: "YouTube",
    category: "Media",
    url: "https://youtube.com",
    icon: <YoutubeIcon />,
  },
  {
    id: "x",
    label: "X",
    category: "Feed",
    url: "https://x.com",
    icon: <XIcon />,
  },
  {
    id: "add",
    label: "Add",
    category: "Shortcut",
    url: "#add",
    icon: <Plus size={20} strokeWidth={2.2} />,
  },
];

interface NewTabProps {
  onNavigate: (url: string) => void;
  onToggleAi?: () => void;
}

export const NewTab: React.FC<NewTabProps> = ({ onNavigate, onToggleAi }) => {
  const [query, setQuery] = useState("");
  const [currentTime, setCurrentTime] = useState(new Date());
  const videoRef = useRef<HTMLVideoElement>(null);

  // Keep clock live every second
  useEffect(() => {
    const timer = setInterval(() => setCurrentTime(new Date()), 1000);
    return () => clearInterval(timer);
  }, []);

  // Pause the wallpaper video if the user prefers reduced motion
  useEffect(() => {
    const mq = window.matchMedia("(prefers-reduced-motion: reduce)");
    const apply = (e: MediaQueryList | MediaQueryListEvent) => {
      if (!videoRef.current) return;
      if (e.matches) {
        videoRef.current.pause();
      } else {
        videoRef.current.play().catch(() => {/* autoplay policy: silently ignore */});
      }
    };
    apply(mq);
    mq.addEventListener("change", apply);
    return () => mq.removeEventListener("change", apply);
  }, []);

  const timeStr = currentTime.toLocaleTimeString([], {
    hour: "2-digit",
    minute: "2-digit",
  });

  const dateStr = currentTime.toLocaleDateString([], {
    weekday: "short",
    day: "numeric",
    month: "short",
    year: "numeric",
  });

  const handleSearch = (e: React.FormEvent) => {
    e.preventDefault();
    if (!query.trim()) return;
    const isUrl = /^https?:\/\//.test(query) || /^[\w-]+\.[\w.]+/.test(query);
    onNavigate(isUrl ? query : `https://www.google.com/search?q=${encodeURIComponent(query)}`);
  };

  return (
    <div className="new-tab" role="main">
      {/* ── Live wallpaper — muted, looping forever ───────────────── */}
      <video
        ref={videoRef}
        className="new-tab__wallpaper"
        src="/livewallpaper.mp4"
        autoPlay
        muted
        loop
        playsInline
        disablePictureInPicture
        aria-hidden="true"
        tabIndex={-1}
      />

      {/* ── Left Vertical Philosophy Pillar ───────────────────────── */}
      <aside className="new-tab__pillar" aria-label="Brand Philosophy">
        <div className="new-tab__pillar-jp" lang="ja">
          つくる、もっと自由に。
        </div>
        <div className="new-tab__pillar-divider" />
        <div className="new-tab__pillar-en">
          BUILD · DEBUG · EXPLORE · REPEAT
        </div>
      </aside>

      {/* ── Top-Right Status Widget (No emojis) ───────────────────── */}
      <div className="new-tab__top-right" aria-live="off">
        <div className="new-tab__weather">
          <Sun size={15} strokeWidth={2.2} className="new-tab__weather-icon" />
          <span className="new-tab__weather-temp">28°C</span>
          <span className="new-tab__weather-loc">New Delhi</span>
        </div>
        <time className="new-tab__clock-time" dateTime={currentTime.toISOString()}>
          {timeStr}
        </time>
        <time className="new-tab__clock-date" dateTime={currentTime.toDateString()}>
          {dateStr}
        </time>
        <div className="new-tab__clock-quote">
          SAME WEB. A HIGHER MINDSET.
        </div>
      </div>

      {/* ── Center Stage ─────────────────────────────────────────── */}
      <div className="new-tab__center">
        {/* Main Logo Only — No center text replacement */}
        <div className="new-tab__hero">
          <div className="new-tab__logo-wrap">
            <img src="/Logo.png" alt="KAGE" className="new-tab__logo-img" />
          </div>
        </div>

        {/* Liquid Glass Search Capsule */}
        <form
          className="new-tab__search-form"
          onSubmit={handleSearch}
          role="search"
          aria-label="Web search"
        >
          <div className="new-tab__search-wrap">
            <Search
              size={18}
              strokeWidth={2}
              className="new-tab__search-icon"
              aria-hidden="true"
            />
            <input
              id="new-tab-search"
              type="search"
              className="new-tab__search-input"
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              placeholder="Search the web or ask Kage AI…"
              aria-label="Search or enter a URL"
              autoFocus
              spellCheck={false}
              autoComplete="off"
            />
            <div className="new-tab__search-actions">
              <button
                type="button"
                className="new-tab__search-action-btn new-tab__search-ai-btn"
                onClick={onToggleAi}
                aria-label="Ask Kage AI"
                title="Ask Kage AI"
              >
                <Sparkles size={16} strokeWidth={2} />
              </button>
              <span className="new-tab__search-divider" />
              <button
                type="button"
                className="new-tab__search-action-btn"
                aria-label="Voice Search"
                title="Voice Search"
              >
                <Mic size={16} strokeWidth={2} />
              </button>
            </div>
          </div>
        </form>

        {/* Quick Launch Cards (6 cards with visible crisp brand icons) */}
        <div className="new-tab__quick-grid" role="list" aria-label="Quick launch bookmarks">
          {QUICK_LINKS.map((link) => (
            <button
              key={link.id}
              className={`new-tab__quick-card ${link.id === "add" ? "new-tab__quick-card--add" : ""}`}
              onClick={() => {
                if (link.id !== "add") onNavigate(link.url);
              }}
              aria-label={`Open ${link.label} (${link.category})`}
              id={`quick-link-${link.id}`}
              role="listitem"
            >
              <div className="new-tab__quick-icon-badge">
                {link.icon}
              </div>
              <div className="new-tab__quick-info">
                <span className="new-tab__quick-title">{link.label}</span>
                <span className="new-tab__quick-category">{link.category}</span>
              </div>
            </button>
          ))}
        </div>
      </div>

      {/* ── Bottom-Right Motto Card ───────────────────────────────── */}
      <footer className="new-tab__bottom-right" aria-label="KAGE Motto">
        <span className="new-tab__motto-badge">
          A FASTER WEB FOR A BRIGHTER TOMORROW.
        </span>
      </footer>
    </div>
  );
};
