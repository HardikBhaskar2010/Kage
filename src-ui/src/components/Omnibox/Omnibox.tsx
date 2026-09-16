import React, { useId } from "react";
import "./Omnibox.css";

interface OmniboxProps {
  url: string;
  onUrlChange: (url: string) => void;
  onNavigate: (url: string) => void;
  isSecure?: boolean;
  onFavourite?: () => void;
  onRefresh?: () => void;
  onBack?: () => void;
  onForward?: () => void;
  canGoBack?: boolean;
  canGoForward?: boolean;
}

const LockIcon = () => (
  <svg width="12" height="12" viewBox="0 0 12 12" fill="none" aria-hidden="true">
    <rect x="2" y="5" width="8" height="6" rx="1.5" stroke="currentColor" strokeWidth="1.3"/>
    <path d="M4 5V3.5a2 2 0 0 1 4 0V5" stroke="currentColor" strokeWidth="1.3" strokeLinecap="round"/>
  </svg>
);

const StarIcon = () => (
  <svg width="14" height="14" viewBox="0 0 14 14" fill="none" aria-hidden="true">
    <path d="M7 1.5l1.545 3.13L12 5.2l-2.5 2.435.59 3.44L7 9.5l-3.09 1.575L4.5 7.635 2 5.2l3.455-.57L7 1.5z" stroke="currentColor" strokeWidth="1.2" strokeLinejoin="round"/>
  </svg>
);

const RefreshIcon = () => (
  <svg width="14" height="14" viewBox="0 0 14 14" fill="none" aria-hidden="true">
    <path d="M11.5 7A4.5 4.5 0 1 1 9.6 3.4l1.9-1.4" stroke="currentColor" strokeWidth="1.3" strokeLinecap="round" strokeLinejoin="round"/>
    <path d="M8 2h3.5V5.5" stroke="currentColor" strokeWidth="1.3" strokeLinecap="round" strokeLinejoin="round"/>
  </svg>
);

const BackIcon = () => (
  <svg width="14" height="14" viewBox="0 0 14 14" fill="none" aria-hidden="true">
    <path d="M9 11L5 7l4-4" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round" strokeLinejoin="round"/>
  </svg>
);

const ForwardIcon = () => (
  <svg width="14" height="14" viewBox="0 0 14 14" fill="none" aria-hidden="true">
    <path d="M5 3l4 4-4 4" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round" strokeLinejoin="round"/>
  </svg>
);

export const Omnibox: React.FC<OmniboxProps> = ({
  url,
  onUrlChange,
  onNavigate,
  isSecure = true,
  onFavourite,
  onRefresh,
  onBack,
  onForward,
  canGoBack = false,
  canGoForward = false,
}) => {
  const inputId = useId();

  const handleKeyDown = (e: React.KeyboardEvent<HTMLInputElement>) => {
    if (e.key === "Enter") {
      onNavigate(url);
    }
  };

  return (
    <div className="omnibox" role="navigation" aria-label="Browser navigation bar">
      {/* Nav buttons */}
      <div className="omnibox__nav-btns">
        <button
          className="omnibox__nav-btn"
          onClick={onBack}
          disabled={!canGoBack}
          aria-label="Go back"
          id="btn-nav-back"
          title="Back"
        >
          <BackIcon />
        </button>
        <button
          className="omnibox__nav-btn"
          onClick={onForward}
          disabled={!canGoForward}
          aria-label="Go forward"
          id="btn-nav-forward"
          title="Forward"
        >
          <ForwardIcon />
        </button>
        <button
          className="omnibox__nav-btn"
          onClick={onRefresh}
          aria-label="Refresh page"
          id="btn-nav-refresh"
          title="Refresh"
        >
          <RefreshIcon />
        </button>
      </div>

      {/* URL input */}
      <div className="omnibox__input-wrap">
        <span className={`omnibox__lock ${isSecure ? "omnibox__lock--secure" : "omnibox__lock--warn"}`} aria-label={isSecure ? "Secure connection" : "Not secure"}>
          <LockIcon />
        </span>
        <input
          id={inputId}
          type="text"
          className="omnibox__input"
          value={url}
          onChange={(e) => onUrlChange(e.target.value)}
          onKeyDown={handleKeyDown}
          aria-label="Address bar — enter a URL or search query"
          spellCheck={false}
          autoComplete="off"
        />
        <button
          className="omnibox__action-btn"
          onClick={onFavourite}
          aria-label="Add to favourites"
          id="btn-favourite"
          title="Favourite"
        >
          <StarIcon />
        </button>
      </div>
    </div>
  );
};
