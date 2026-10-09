"use client";

import { useState } from "react";
import Sidebar from "@components/Sidebar";
import RulesTab from "@components/Rules";
import AlertsTab from "@components/Alerts";
import SourcesTab from "@components/Sources";
import DestinationsTab from "@components/Destinations";
import StorageTab from "@components/Storage";
import NotificationsTab from "@components/Notifications";
import ExploreTab from "@components/Explore";
import DashboardTab from "@components/Dashboard";

export default function Home() {
  const [activeTab, setActiveTab] = useState("detections");

  return (
    <div className="font-sans flex h-screen bg-gray-100">
      <Sidebar
        activeTab={activeTab}
        onTabChange={setActiveTab}
      />
      <main className="flex-1 overflow-hidden">
        {activeTab === "detections" && <RulesTab />}
        {activeTab === "alerts" && <AlertsTab />}
        {activeTab === "sources" && <SourcesTab />}
        {activeTab === "destinations" && <DestinationsTab />}
        {activeTab === "storage" && <StorageTab />}
        {activeTab === "notifications" && <NotificationsTab />}
        {activeTab === "explore" && <ExploreTab />}
        {activeTab === "dashboard" && <DashboardTab />}
      </main>
    </div>
  );
}
