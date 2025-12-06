"use client";

import { useState } from "react";
import { useFeatureFlags } from "@/include/features";
import Sidebar from "@components/Sidebar";
import RulesTab from "@components/Rules";
import AlertsTab from "@components/Alerts";
import SourcesTab from "@components/Sources";
import StorageTab from "@components/Storage";
import ExploreTab from "@components/Explore";

export default function Home() {
  const [activeTab, setActiveTab] = useState("detections");
  const { features, hasFeature } = useFeatureFlags();

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
        {activeTab === "storage" && <StorageTab />}
        {activeTab === "explore" && <ExploreTab />}
      </main>
    </div>
  );
}
