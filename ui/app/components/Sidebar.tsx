"use client";

import ThemeToggle from "@components/ThemeToggle";

interface SidebarProps {
  activeTab: string;
  onTabChange: (tab: string) => void;
}

interface Tab {
  id: string;
  label: string;
  icon: string;
}

interface Section {
  label: string;
  tabs: Tab[];
}

const sections: Section[] = [
  {
    label: "Observability",
    tabs: [{ id: "dashboard", label: "Dashboard", icon: "📊" }],
  },
  {
    label: "Data",
    tabs: [
      { id: "sources", label: "Sources", icon: "🔗" },
      { id: "destinations", label: "Destinations", icon: "🗄️" },
      { id: "storage", label: "Storage", icon: "💾" },
      { id: "explore", label: "Explore", icon: "🔍" },
    ],
  },
  {
    label: "Alerting",
    tabs: [
      { id: "alerts", label: "Alerts", icon: "🚨" },
      { id: "detections", label: "Detections", icon: "📋" },
      { id: "notifications", label: "Notifications", icon: "🔔" },
    ],
  },
];

export default function Sidebar({ activeTab, onTabChange }: SidebarProps) {
  const renderTab = (tab: Tab) => (
    <button
      key={tab.id}
      onClick={() => onTabChange(tab.id)}
      className={`nav-button ${
        activeTab === tab.id ? "nav-button-active" : "nav-button-inactive"
      }`}
    >
      <span className="text-lg">{tab.icon}</span>
      <span className="font-medium">{tab.label}</span>
    </button>
  );

  return (
    <nav className="sidebar">
      <div className="sidebar-header">
        <h1 className="text-2xl font-semibold">StrIEM</h1>
      </div>
      <div className="sidebar-nav">
        {sections.map((section) => (
          <div key={section.label}>
            <div className="sidebar-section-label">{section.label}</div>
            {section.tabs.map(renderTab)}
          </div>
        ))}
      </div>
      <ThemeToggle />
    </nav>
  );
}
