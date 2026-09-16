import React, { useState, useRef, useEffect, useId } from "react";
import "./AISidebar.css";

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

const SendIcon = () => (
  <svg width="16" height="16" viewBox="0 0 16 16" fill="none" aria-hidden="true">
    <path d="M14 8L2 2l3 6-3 6 12-6z" fill="currentColor"/>
  </svg>
);

const CloseIcon = () => (
  <svg width="14" height="14" viewBox="0 0 14 14" fill="none" aria-hidden="true">
    <path d="M2 2l10 10M12 2L2 12" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round"/>
  </svg>
);

const KageAvatar = () => (
  <div className="ai-avatar" aria-hidden="true">
    <img
      src="/Logo.png"
      alt=""
      width={28}
      height={28}
      className="ai-avatar__img"
    />
  </div>
);

export const AISidebar: React.FC<AISidebarProps> = ({
  isOpen,
  onClose,
  stepCount = 0,
  maxSteps = 10,
}) => {
  const [input, setInput] = useState("");
  const [messages, setMessages] = useState<Message[]>([
    {
      id: "welcome",
      role: "assistant",
      text: "Hi, I'm Kage AI.\nI can help you with:",
      timestamp: new Date(),
    },
  ]);
  const [isThinking, setIsThinking] = useState(false);
  const messagesEndRef = useRef<HTMLDivElement>(null);
  const inputId = useId();

  useEffect(() => {
    messagesEndRef.current?.scrollIntoView({ behavior: "smooth" });
  }, [messages]);

  const handleSend = () => {
    const trimmed = input.trim();
    if (!trimmed || isThinking) return;

    const userMsg: Message = {
      id: `msg-${Date.now()}`,
      role: "user",
      text: trimmed,
      timestamp: new Date(),
    };
    setMessages((prev) => [...prev, userMsg]);
    setInput("");
    setIsThinking(true);

    // Stub: real dispatch goes through ToolBus IPC
    setTimeout(() => {
      setMessages((prev) => [
        ...prev,
        {
          id: `msg-${Date.now()}-ai`,
          role: "assistant",
          text: "Analysing page context… (Tool Bus connected in Chunk 7)",
          timestamp: new Date(),
        },
      ]);
      setIsThinking(false);
    }, 900);
  };

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === "Enter" && !e.shiftKey) {
      e.preventDefault();
      handleSend();
    }
  };

  const stepFraction = stepCount / maxSteps;
  const stepPct = Math.round(stepFraction * 100);

  return (
    <aside
      className={`ai-sidebar glass-panel ${isOpen ? "ai-sidebar--open" : ""}`}
      aria-label="AI Developer Assistant"
      aria-hidden={!isOpen}
      id="ai-sidebar"
    >
      {/* Header */}
      <div className="ai-sidebar__header">
        <div className="ai-sidebar__title-row">
          <KageAvatar />
          <div className="ai-sidebar__title-text">
            <span className="ai-sidebar__title">AI Developer Assistant</span>
            <span className="ai-sidebar__subtitle">Analyze. Debug. Optimize.</span>
          </div>
        </div>
        <button
          className="ai-sidebar__close"
          onClick={onClose}
          aria-label="Close AI sidebar"
          id="btn-ai-close"
        >
          <CloseIcon />
        </button>
      </div>

      {/* Step counter — bounded agent execution tracker */}
      {stepCount > 0 && (
        <div className="ai-sidebar__steps" role="status" aria-label={`Agent step ${stepCount} of ${maxSteps}`}>
          <div className="ai-steps__label">
            <span>Agent steps</span>
            <span className={stepPct > 80 ? "ai-steps__count--warn" : ""}>{stepCount}/{maxSteps}</span>
          </div>
          <div className="ai-steps__bar" role="progressbar" aria-valuenow={stepCount} aria-valuemin={0} aria-valuemax={maxSteps}>
            <div
              className="ai-steps__fill"
              style={{ width: `${stepPct}%` }}
            />
          </div>
        </div>
      )}

      {/* Quick actions */}
      <div className="ai-sidebar__quick-actions" role="group" aria-label="Quick AI actions">
        {["Analyze Page", "Explain", "Generate"].map((action) => (
          <button
            key={action}
            className="ai-quick-btn"
            id={`btn-ai-${action.toLowerCase().replace(" ", "-")}`}
            aria-label={action}
          >
            {action}
          </button>
        ))}
      </div>

      {/* Messages */}
      <div
        className="ai-sidebar__messages"
        role="log"
        aria-live="polite"
        aria-label="AI conversation"
      >
        {messages.map((msg) => (
          <div
            key={msg.id}
            className={`ai-message ai-message--${msg.role}`}
          >
            {msg.role === "assistant" && (
              <span className="ai-message__label" aria-hidden="true">Kage AI</span>
            )}
            <p className="ai-message__text">{msg.text}</p>
          </div>
        ))}
        {isThinking && (
          <div className="ai-message ai-message--assistant" aria-live="polite" aria-label="Kage AI is thinking">
            <span className="ai-message__label" aria-hidden="true">Kage AI</span>
            <div className="ai-thinking" aria-hidden="true">
              <span/><span/><span/>
            </div>
          </div>
        )}
        <div ref={messagesEndRef} />
      </div>

      {/* Suggestion chips */}
      <div className="ai-sidebar__chips" role="group" aria-label="Suggested actions">
        {[
          "Analyze this page",
          "Explain selected element",
          "Find issues",
          "Generate test",
        ].map((chip) => (
          <button
            key={chip}
            className="ai-chip"
            onClick={() => setInput(chip)}
            aria-label={chip}
          >
            {chip}
          </button>
        ))}
      </div>

      {/* Input */}
      <div className="ai-sidebar__input-area">
        <label htmlFor={inputId} className="sr-only">
          Ask Kage AI anything about this page
        </label>
        <textarea
          id={inputId}
          className="ai-sidebar__input"
          value={input}
          onChange={(e) => setInput(e.target.value)}
          onKeyDown={handleKeyDown}
          placeholder="Ask anything about this page…"
          rows={1}
          aria-label="AI prompt input"
          disabled={isThinking}
        />
        <button
          className="ai-sidebar__send"
          onClick={handleSend}
          disabled={!input.trim() || isThinking}
          aria-label="Send message"
          id="btn-ai-send"
        >
          <SendIcon />
        </button>
      </div>
    </aside>
  );
};
