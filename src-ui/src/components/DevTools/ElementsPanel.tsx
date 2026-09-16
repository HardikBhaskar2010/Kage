import React, { useState } from "react";
import "./ElementsPanel.css";
import { useBrowser } from "../../context/BrowserContext";
import type { DOMNode } from "../../context/BrowserContext";
import { ChevronRight, ChevronDown } from "lucide-react";

interface DOMTreeNodeProps {
  node: DOMNode;
  depth: number;
  selectedId: string | null;
  onSelect: (node: DOMNode) => void;
}

const DOMTreeNode: React.FC<DOMTreeNodeProps> = ({ node, depth, selectedId, onSelect }) => {
  const [collapsed, setCollapsed] = useState(false);
  const hasChildren = node.children && node.children.length > 0;
  const isSelected = selectedId === node.id;

  return (
    <div className="dom-node-wrap">
      <div
        className={`dom-node-line ${isSelected ? "dom-node-line--selected" : ""}`}
        style={{ paddingLeft: `${depth * 14 + 6}px` }}
        onClick={() => onSelect(node)}
      >
        {hasChildren ? (
          <button
            type="button"
            className="dom-node-toggle"
            onClick={(e) => {
              e.stopPropagation();
              setCollapsed(!collapsed);
            }}
          >
            {collapsed ? <ChevronRight size={11} strokeWidth={2} /> : <ChevronDown size={11} strokeWidth={2} />}
          </button>
        ) : (
          <span className="dom-node-spacer" />
        )}

        <span className="dom-tag">&lt;{node.tag}</span>
        {node.className && (
          <span className="dom-attr">
            {" "}class=<span className="dom-attr-val">"{node.className}"</span>
          </span>
        )}
        {Object.entries(node.attributes).map(([k, v]) => (
          <span key={k} className="dom-attr">
            {" "}{k}=<span className="dom-attr-val">"{v}"</span>
          </span>
        ))}
        <span className="dom-tag">&gt;</span>

        {node.text && <span className="dom-text">{node.text}</span>}

        {!hasChildren && <span className="dom-tag">&lt;/{node.tag}&gt;</span>}
      </div>

      {hasChildren && !collapsed && (
        <div className="dom-node-children">
          {node.children!.map((child) => (
            <DOMTreeNode
              key={child.id}
              node={child}
              depth={depth + 1}
              selectedId={selectedId}
              onSelect={onSelect}
            />
          ))}
          <div
            className="dom-node-close-line"
            style={{ paddingLeft: `${depth * 14 + 6 + 14}px` }}
          >
            <span className="dom-tag">&lt;/{node.tag}&gt;</span>
          </div>
        </div>
      )}
    </div>
  );
};

export const ElementsPanel: React.FC = () => {
  const { activeDomTree, selectedDomNodeId, setSelectedDomNodeId } = useBrowser();
  const [activeNode, setActiveNode] = useState<DOMNode | null>(null);

  const handleSelect = (node: DOMNode) => {
    setSelectedDomNodeId(node.id);
    setActiveNode(node);
  };

  const box = activeNode?.boxModel || { margin: 0, border: 0, padding: 0, width: 1440, height: 900 };

  return (
    <div className="elements-panel" role="region" aria-label="DOM Elements Inspector">
      {/* ── Left: DOM Tree Explorer ─────────────────────────────── */}
      <div className="elements-tree-pane">
        <div className="elements-pane-header">
          <span>DOM Tree Hierarchy</span>
          <span className="elements-hint">Click a node to inspect styles & box model</span>
        </div>
        <div className="elements-tree-scroll">
          <DOMTreeNode
            node={activeDomTree}
            depth={0}
            selectedId={selectedDomNodeId}
            onSelect={handleSelect}
          />
        </div>
      </div>

      {/* ── Right: Computed Styles & Box Model ──────────────────── */}
      <div className="elements-style-pane">
        <div className="elements-pane-header">
          <span>Computed Box Model & Styles</span>
        </div>

        <div className="elements-style-scroll">
          {/* Box Model Diagram */}
          <div className="box-model-container">
            <div className="bm-margin">
              <span className="bm-label">margin</span>
              <span className="bm-val bm-val--top">{box.margin}</span>
              <span className="bm-val bm-val--left">{box.margin}</span>
              <span className="bm-val bm-val--right">{box.margin}</span>
              <span className="bm-val bm-val--bottom">{box.margin}</span>

              <div className="bm-border">
                <span className="bm-label">border</span>
                <span className="bm-val bm-val--top">{box.border}</span>
                <span className="bm-val bm-val--left">{box.border}</span>
                <span className="bm-val bm-val--right">{box.border}</span>
                <span className="bm-val bm-val--bottom">{box.border}</span>

                <div className="bm-padding">
                  <span className="bm-label">padding</span>
                  <span className="bm-val bm-val--top">{box.padding}</span>
                  <span className="bm-val bm-val--left">{box.padding}</span>
                  <span className="bm-val bm-val--right">{box.padding}</span>
                  <span className="bm-val bm-val--bottom">{box.padding}</span>

                  <div className="bm-content">
                    <span>{box.width} × {box.height}</span>
                  </div>
                </div>
              </div>
            </div>
          </div>

          {/* Computed CSS Rules */}
          <div className="elements-css-section">
            <h4 className="elements-css-title">
              {activeNode ? `<${activeNode.tag}${activeNode.className ? `.${activeNode.className}` : ""}>` : "Selected Element Styles"}
            </h4>
            <div className="elements-css-rules">
              {activeNode?.styles ? (
                Object.entries(activeNode.styles).map(([prop, val]) => (
                  <div key={prop} className="elements-css-row">
                    <span className="css-prop">{prop}:</span>
                    <span className="css-val"> {val};</span>
                  </div>
                ))
              ) : (
                <>
                  <div className="elements-css-row"><span className="css-prop">display:</span><span className="css-val"> flex;</span></div>
                  <div className="elements-css-row"><span className="css-prop">box-sizing:</span><span className="css-val"> border-box;</span></div>
                  <div className="elements-css-row"><span className="css-prop">color:</span><span className="css-val"> var(--color-accent-peach);</span></div>
                  <div className="elements-css-row"><span className="css-prop">font-family:</span><span className="css-val"> Inter, sans-serif;</span></div>
                </>
              )}
            </div>
          </div>
        </div>
      </div>
    </div>
  );
};
