"use client";

import { useFeatureFlags } from "@/include/features";

interface SidebarProps {
  activeTab: string;
  onTabChange: (tab: string) => void;
}

export default function Sidebar({ activeTab, onTabChange }: SidebarProps) {
  const { hasFeature } = useFeatureFlags();
  const hasDuckDB = true;

  const tabs = [
    { id: "detections", label: "Sigma Rules", icon: "📋" },
    { id: "alerts", label: "Alerts", icon: "🚨" },
    { id: "sources", label: "Sources", icon: "🔗" },
    { id: "storage", label: "Storage", icon: "💾" },
    ...(hasDuckDB ? [{ id: "explore", label: "Explore", icon: "🔍" }] : []),
  ];

  return (
    <nav className="sidebar">
      <div className="sidebar-header">
        <h1 className="text-2xl font-semibold">StrIEM</h1>
      </div>
      <div className="sidebar-nav">
        {tabs.map((tab) => (
          <button
            key={tab.id}
            onClick={() => onTabChange(tab.id)}
            className={`nav-button ${
              activeTab === tab.id
                ? "nav-button-active"
                : "nav-button-inactive"
            }`}
          >
            <span className="text-lg">{tab.icon}</span>
            <span className="font-medium">{tab.label}</span>
          </button>
        ))}
      </div>
    </nav>
  );
}
