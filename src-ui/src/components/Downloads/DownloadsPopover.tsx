import React from "react";
import "./DownloadsPopover.css";
import { useBrowser } from "../../context/BrowserContext";
import { Download, CheckCircle, Folder, X } from "lucide-react";

export const DownloadsPopover: React.FC = () => {
  const { downloads, downloadsOpen, setDownloadsOpen } = useBrowser();

  if (!downloadsOpen) return null;

  return (
    <div className="downloads-popover glass-panel" role="dialog" aria-label="Downloads">
      <div className="dl-header">
        <div className="dl-title">
          <Download size={14} strokeWidth={2} />
          <span>Downloads</span>
        </div>
        <button
          className="dl-close-btn"
          onClick={() => setDownloadsOpen(false)}
          aria-label="Close downloads"
        >
          <X size={13} strokeWidth={2} />
        </button>
      </div>

      <div className="dl-list">
        {downloads.length === 0 ? (
          <div className="dl-empty">No active downloads</div>
        ) : (
          downloads.map((item) => (
            <div key={item.id} className="dl-item">
              <div className="dl-item__icon">
                {item.status === "completed" ? (
                  <CheckCircle size={16} strokeWidth={2} className="dl-icon--done" />
                ) : (
                  <Download size={16} strokeWidth={2} className="dl-icon--active" />
                )}
              </div>

              <div className="dl-item__info">
                <span className="dl-item__name">{item.filename}</span>
                <div className="dl-item__sub">
                  <span>{item.size}</span>
                  <span>·</span>
                  <span>{item.speed}</span>
                </div>

                {item.status === "downloading" && (
                  <div className="dl-progress-bar">
                    <div
                      className="dl-progress-fill"
                      style={{ width: `${item.progress}%` }}
                    />
                  </div>
                )}
              </div>

              <button className="dl-folder-btn" title="Show in folder">
                <Folder size={13} strokeWidth={2} />
              </button>
            </div>
          ))
        )}
      </div>
    </div>
  );
};
