import React, { useState, useRef, useEffect, useId } from "react";
import "./AISidebar.css";
import {
  X,
  ChevronRight,
  ChevronDown,
  Paperclip,
  SendHorizontal,
  BarChart3,
  Crosshair,
  Bug,
  Zap,
  FlaskConical,
  MessageSquare,
} from "lucide-react";
import { aiProviderRegistry } from "../../services/aiProviderDiscovery";

interface Message {
  id: string;
  role: "user" | "assistant";
  text: string;
  timestamp: Date;
}

interface AISidebarProps {
  isOpen: boolean;
  onClose: () => void;
  stepCount?: number;
  maxSteps?: number;
}

const ACTION_CARDS = [
  {
    id: "analyze",
    icon: <BarChart3 size={18} strokeWidth={1.8} />,
    title: "Analyze this page",
    desc: "Get a full performance, SEO and accessibility report",
  },
  {
    id: "explain",
    icon: <Crosshair size={18} strokeWidth={1.8} />,
    title: "Explain selected element",
    desc: "Understand how it works",
  },
  {
    id: "issues",
    icon: <Bug size={18} strokeWidth={1.8} />,
    title: "Find issues",
    desc: "Scan for common problems",
  },
  {
    id: "test",
    icon: <FlaskConical size={18} strokeWidth={1.8} />,
    title: "Generate test",
    desc: "Create a Playwright test from this page",
  },
];

export const AISidebar: React.FC<AISidebarProps> = ({
  isOpen,
  onClose,
}) => {
  const [input, setInput] = useState("");
  const [model, setModel] = useState("GPT-4o");
  const [showModelPicker, setShowModelPicker] = useState(false);
  const [messages, setMessages] = useState<Message[]>([]);
  const [isThinking, setIsThinking] = useState(false);
  const messagesEndRef = useRef<HTMLDivElement>(null);
  const inputId = useId();

  useEffect(() => {
    if (isOpen && messages.length > 0) {
      messagesEndRef.current?.scrollIntoView({ behavior: "smooth", block: "nearest", inline: "nearest" });
    }
  }, [messages, isOpen]);

  const handleSend = (textToSend?: string) => {
    const text = (textToSend ?? input).trim();
    if (!text || isThinking) return;

    const userMsg: Message = {
      id: `msg-${Date.now()}`,
      role: "user",
      text,
      timestamp: new Date(),
    };
    setMessages((prev) => [...prev, userMsg]);
    if (!textToSend) setInput("");
    setIsThinking(true);

    setTimeout(() => {
      setMessages((prev) => [
        ...prev,
        {
          id: `msg-${Date.now()}-ai`,
          role: "assistant",
          text: `Analyzing context for "${text}" with ${model}… Tool Bus dispatcher ready.`,
          timestamp: new Date(),
        },
      ]);
      setIsThinking(false);
    }, 850);
  };

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === "Enter" && !e.shiftKey) {
      e.preventDefault();
      handleSend();
    }
  };

  return (
    <aside
      className={`ai-sidebar ${isOpen ? "ai-sidebar--open" : ""}`}
      aria-label="KAGE AI Developer Assistant"
      aria-hidden={!isOpen}
      id="ai-sidebar"
    >
      {/* ── Header ────────────────────────────────────────────── */}
      <div className="ai-sidebar__header">
        <div className="ai-sidebar__header-title">
          <span className="ai-sidebar__brand-name">K A G E   A I</span>
        </div>
        <button
          className="ai-sidebar__close-btn"
          onClick={onClose}
          aria-label="Close AI Sidebar"
          id="btn-ai-close"
        >
          <X size={15} strokeWidth={2} />
        </button>
      </div>

      <div className="ai-sidebar__scroll-body">
        {/* ── Copilot Profile Card ─────────────────────────────── */}
        <div className="ai-copilot-card">
          <div className="ai-copilot-card__avatar-wrap">
            <img src="/avatar.png" alt="Kage AI Avatar" className="ai-copilot-card__avatar" />
            <span className="ai-copilot-card__status-dot" title="Online" />
          </div>
          <div className="ai-copilot-card__meta">
            <h3 className="ai-copilot-card__title">Your Developer Copilot for the Web.</h3>
            <p className="ai-copilot-card__subtitle">Understand. Debug. Optimize.</p>
          </div>
        </div>

        {/* ── Quick Action Pill Buttons ────────────────────────── */}
        <div className="ai-sidebar__pills" role="group" aria-label="AI shortcut actions">
          {["Analyze Page", "Explain", "Generate"].map((act) => (
            <button
              key={act}
              className="ai-pill-btn"
              onClick={() => handleSend(act)}
            >
              {act}
            </button>
          ))}
        </div>

        {/* ── Chat Messages (if conversation started) ─────────── */}
        {messages.length > 0 && (
          <div className="ai-sidebar__conversation">
            {messages.map((m) => (
              <div key={m.id} className={`ai-chat-bubble ai-chat-bubble--${m.role}`}>
                <span className="ai-chat-bubble__sender">{m.role === "assistant" ? "Kage AI" : "You"}</span>
                <p className="ai-chat-bubble__text">{m.text}</p>
              </div>
            ))}
            {isThinking && (
              <div className="ai-chat-bubble ai-chat-bubble--assistant">
                <span className="ai-chat-bubble__sender">Kage AI</span>
                <div className="ai-thinking-indicator">
                  <span /><span /><span />
                </div>
              </div>
            )}
            <div ref={messagesEndRef} />
          </div>
        )}

        {/* ── Default Capability & Action List (NO EMOJIS) ──────── */}
        {messages.length === 0 && (
          <>
            <div className="ai-intro-section">
              <p className="ai-intro-section__greeting">
                Hi, I'm Kage AI.<br />
                I can help you with:
              </p>
              <ul className="ai-capabilities-list">
                <li><span className="ai-cap-icon"><BarChart3 size={15} strokeWidth={1.8} /></span> Analyze any website</li>
                <li><span className="ai-cap-icon"><Crosshair size={15} strokeWidth={1.8} /></span> Inspect elements</li>
                <li><span className="ai-cap-icon"><Bug size={15} strokeWidth={1.8} /></span> Debug errors</li>
                <li><span className="ai-cap-icon"><Zap size={15} strokeWidth={1.8} /></span> Optimize performance</li>
                <li><span className="ai-cap-icon"><FlaskConical size={15} strokeWidth={1.8} /></span> Generate tests</li>
                <li><span className="ai-cap-icon"><MessageSquare size={15} strokeWidth={1.8} /></span> Answer your questions</li>
              </ul>
            </div>

            <div className="ai-action-cards-section">
              <h4 className="ai-action-cards__heading">What would you like to do?</h4>
              <div className="ai-action-cards-list">
                {ACTION_CARDS.map((card) => (
                  <button
                    key={card.id}
                    className="ai-action-card"
                    onClick={() => handleSend(card.title)}
                  >
                    <div className="ai-action-card__icon-box">
                      {card.icon}
                    </div>
                    <div className="ai-action-card__info">
                      <span className="ai-action-card__title">{card.title}</span>
                      <span className="ai-action-card__desc">{card.desc}</span>
                    </div>
                    <ChevronRight size={15} strokeWidth={2} className="ai-action-card__arrow" />
                  </button>
                ))}
              </div>
            </div>
          </>
        )}
      </div>

      {/* ── Bottom Input & Footer ───────────────────────────────── */}
      <div className="ai-sidebar__bottom-panel">
        <div className="ai-input-capsule">
          <textarea
            id={inputId}
            className="ai-input-capsule__textarea"
            placeholder="Ask anything about this page..."
            value={input}
            onChange={(e) => setInput(e.target.value)}
            onKeyDown={handleKeyDown}
            rows={1}
          />
          <div className="ai-input-capsule__toolbar">
            <div className="ai-input-capsule__tools-left">
              <button type="button" className="ai-tool-btn" title="Attach file or screenshot">
                <Paperclip size={15} strokeWidth={1.8} />
              </button>

              <div className="ai-model-selector-wrap">
                <button
                  type="button"
                  className="ai-model-btn"
                  onClick={() => setShowModelPicker((p) => !p)}
                >
                  <span>{model}</span>
                  <ChevronDown size={12} strokeWidth={2} />
                </button>
                {showModelPicker && (
                  <div className="ai-model-dropdown">
                    {aiProviderRegistry.getAllModels().map((m) => (
                      <button
                        key={m.id}
                        className={`ai-model-option ${m.name === model ? "ai-model-option--selected" : ""}`}
                        onClick={() => {
                          setModel(m.name);
                          aiProviderRegistry.setActiveModel(m.id);
                          setShowModelPicker(false);
                        }}
                      >
                        <span>{m.name}</span>
                        {m.capabilities.toolCalling && (
                          <span className="model-chip" style={{ fontSize: "9px", padding: "1px 4px" }}>
                            TB
                          </span>
                        )}
                      </button>
                    ))}
                  </div>
                )}
              </div>
            </div>

            <button
              type="button"
              className="ai-send-btn"
              onClick={() => handleSend()}
              disabled={!input.trim() || isThinking}
              aria-label="Send message"
            >
              <SendHorizontal size={14} strokeWidth={2} />
            </button>
          </div>
        </div>

        {/* Brand Quote & Accent Bar */}
        <div className="ai-sidebar__quote-row">
          <p className="ai-sidebar__quote">"Better tools. Brighter developers."</p>
          <div className="ai-sidebar__quote-bar">
            <span className="ai-sidebar__quote-bar-fill" />
          </div>
        </div>
      </div>
    </aside>
  );
};
