import React from "react";
import "./TabStrip.css";

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

const CloseIcon = () => (
  <svg width="12" height="12" viewBox="0 0 12 12" fill="none">
    <path d="M2 2L10 10M10 2L2 10" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round"/>
  </svg>
);

const LoadingSpinner = () => (
  <svg className="tab-spinner" width="14" height="14" viewBox="0 0 14 14" fill="none">
    <circle cx="7" cy="7" r="5.5" stroke="currentColor" strokeWidth="1.5" strokeDasharray="8 16" strokeLinecap="round"/>
  </svg>
);

export const TabStrip: React.FC<TabStripProps> = ({
  tabs,
  onTabSelect,
  onTabClose,
  onNewTab,
}) => {
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
            {tab.isLoading ? (
              <LoadingSpinner />
            ) : tab.favicon ? (
              <img src={tab.favicon} alt="" width={14} height={14} />
            ) : (
              <svg width="14" height="14" viewBox="0 0 14 14" fill="none">
                <rect x="1" y="1" width="12" height="12" rx="2" stroke="currentColor" strokeWidth="1.2"/>
                <path d="M4 5h6M4 7h4" stroke="currentColor" strokeWidth="1.2" strokeLinecap="round"/>
              </svg>
            )}
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
            <CloseIcon />
          </button>
        </div>
      ))}
      <button
        className="tab-new"
        onClick={onNewTab}
        aria-label="Open new tab"
        id="btn-new-tab"
      >
        <svg width="14" height="14" viewBox="0 0 14 14" fill="none">
          <path d="M7 2v10M2 7h10" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round"/>
        </svg>
      </button>
    </div>
  );
};
