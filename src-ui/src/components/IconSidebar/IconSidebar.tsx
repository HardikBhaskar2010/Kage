import React from "react";
import "./IconSidebar.css";

export type SidebarItem = {
  id: string;
  icon: React.ReactNode;
  label: string;
  isActive?: boolean;
};

interface IconSidebarProps {
  items: SidebarItem[];
  bottomItems?: SidebarItem[];
  onSelect: (id: string) => void;
  activeId?: string;
}

export const IconSidebar: React.FC<IconSidebarProps> = ({
  items,
  bottomItems = [],
  onSelect,
  activeId,
}) => {
  const renderItem = (item: SidebarItem) => (
    <button
      key={item.id}
      className={`sidebar-item ${activeId === item.id ? "sidebar-item--active" : ""}`}
      onClick={() => onSelect(item.id)}
      aria-label={item.label}
      aria-current={activeId === item.id ? "page" : undefined}
      id={`sidebar-${item.id}`}
      title={item.label}
    >
      <span className="sidebar-item__icon" aria-hidden="true">
        {item.icon}
      </span>
      {/* Visible micro-label below icon — matches mockup */}
      <span className="sidebar-item__label">{item.label}</span>
    </button>
  );

  return (
    <nav
      className="icon-sidebar"
      aria-label="Developer tools navigation"
      role="navigation"
    >
      <div className="icon-sidebar__top">
        {items.map(renderItem)}
      </div>
      {bottomItems.length > 0 && (
        <div className="icon-sidebar__bottom">
          {bottomItems.map(renderItem)}
        </div>
      )}
    </nav>
  );
};
