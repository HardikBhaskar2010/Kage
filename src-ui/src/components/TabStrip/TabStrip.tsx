import React from "react";
import "./TabStrip.css";
import { Home, Plus, X } from "lucide-react";

export interface Tab {
  id: string;
  title: string;
  favicon?: string;
  isActive: boolean;
  isLoading?: boolean;
}

interface TabStripProps {
  tabs: Tab[];
  onTabSelect: (id: string) => void;
  onTabClose: (id: string) => void;
  onNewTab: () => void;
}

const VercelIcon = () => (
  <svg width="12" height="12" viewBox="0 0 16 16" fill="currentColor">
    <path d="M8 2l7 12H1L8 2z"/>
  </svg>
);

const GitHubIcon = () => (
  <svg width="13" height="13" viewBox="0 0 16 16" fill="currentColor">
    <path fillRule="evenodd" d="M8 0C3.58 0 0 3.58 0 8c0 3.54 2.29 6.53 5.47 7.59.4.07.55-.17.55-.38 0-.19-.01-.82-.01-1.49-2.01.37-2.53-.49-2.69-.94-.09-.23-.48-.94-.82-1.13-.28-.15-.68-.52-.01-.53.63-.01 1.08.58 1.23.82.72 1.21 1.87.87 2.33.66.07-.52.28-.87.51-1.07-1.78-.2-3.64-.89-3.64-3.95 0-.87.31-1.59.82-2.15-.08-.2-.36-1.02.08-2.12 0 0 .67-.21 2.2.82.64-.18 1.32-.27 2-.27.68 0 1.36.09 2 .27 1.53-1.04 2.2-.82 2.2-.82.44 1.1.16 1.92.08 2.12.51.56.82 1.27.82 2.15 0 3.07-1.87 3.75-3.65 3.95.29.25.54.73.54 1.48 0 1.07-.01 1.93-.01 2.2 0 .21.15.46.55.38A8.013 8.013 0 0016 8c0-4.42-3.58-8-8-8z"/>
  </svg>
);

const NextJsIcon = () => (
  <svg width="13" height="13" viewBox="0 0 16 16" fill="currentColor">
    <circle cx="8" cy="8" r="7" stroke="currentColor" strokeWidth="1.2" fill="none"/>
    <path d="M5.5 11.5V4.5l6.5 7.5V4.5" stroke="currentColor" strokeWidth="1.2" strokeLinecap="round" strokeLinejoin="round"/>
  </svg>
);

export const TabStrip: React.FC<TabStripProps> = ({
  tabs,
  onTabSelect,
  onTabClose,
  onNewTab,
}) => {
  const getTabIcon = (tab: Tab) => {
    if (tab.title.toLowerCase().includes("new tab")) {
      return <Home size={13} strokeWidth={2} />;
    }
    if (tab.title.toLowerCase().includes("vercel")) {
      return <VercelIcon />;
    }
    if (tab.title.toLowerCase().includes("github")) {
      return <GitHubIcon />;
    }
    if (tab.title.toLowerCase().includes("next")) {
      return <NextJsIcon />;
    }
    return <Home size={13} strokeWidth={2} />;
  };

  return (
    <div className="tab-strip" role="tablist" aria-label="Browser tabs">
      {tabs.map((tab) => (
        <div
          key={tab.id}
          className={`tab ${tab.isActive ? "tab--active" : ""}`}
          role="tab"
          aria-selected={tab.isActive}
          onClick={() => onTabSelect(tab.id)}
          id={`tab-${tab.id}`}
        >
          <span className="tab__favicon">
            {getTabIcon(tab)}
          </span>
          <span className="tab__title">{tab.title}</span>
          <button
            className="tab__close"
            aria-label={`Close tab: ${tab.title}`}
            onClick={(e) => {
              e.stopPropagation();
              onTabClose(tab.id);
            }}
          >
            <X size={10} strokeWidth={2.5} />
          </button>
        </div>
      ))}
      <button
        className="tab-new"
        onClick={onNewTab}
        aria-label="Open new tab"
        id="btn-new-tab"
        title="New Tab"
      >
        <Plus size={15} strokeWidth={2.2} />
      </button>
    </div>
  );
};
