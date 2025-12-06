"use client";

import { useState, useEffect } from "react";
import AddSource from "./AddSource";
import { useFeatureFlags } from "@/include/features";

interface SourceConfig {
  id: string;
  name: string;
  enabled: boolean;
  sourcetype?: string;
}

// Map source types to friendly names
const sourceTypeLabels: Record<string, string> = {
  aws_cloudtrail: "AWS CloudTrail",
  okta: "Okta",
};

export default function SourcesTab() {
  const [sources, setSources] = useState<SourceConfig[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [showAddModal, setShowAddModal] = useState(false);
  const { hasFeature } = useFeatureFlags();

  useEffect(() => {
    loadSources();
  }, []);

  const loadSources = async () => {
    try {
      setLoading(true);
      setError(null);
      const response = await fetch(`${process.env.NEXT_PUBLIC_API_URL}/sources`);
      if (!response.ok) {
        const errorText = await response.text();
        throw new Error(`Failed to fetch sources: ${response.status} ${response.statusText} - ${errorText}`);
      }
      
      const data = await response.json();
      console.log("Sources API Response:", data); // Debug log
      if (!data || !Array.isArray(data)) {
        throw new Error("API response does not contain source_configs array");
      }
      setSources(data);
    } catch (err) {
      console.error("Error loading sources:", err); // Debug log
      setError(err instanceof Error ? err.message : "Unknown error");
    } finally {
      setLoading(false);
    }
  };

  const toggleSource = async (sourceId: string, enabled: boolean) => {
    try {
      const response = await fetch(`${process.env.NEXT_PUBLIC_API_URL}/sources/${encodeURIComponent(sourceId)}/toggle`, {
        method: "PATCH",
        headers: {
          "Content-Type": "application/json",
        },
        body: JSON.stringify({ enabled }),
      });

      if (!response.ok) throw new Error("Failed to toggle source");
      
      setSources(sources.map(source => 
        source.id === sourceId ? { ...source, enabled } : source
      ));
    } catch (err) {
      alert(`Failed to toggle source: ${err instanceof Error ? err.message : "Unknown error"}`);
    }
  };

  const deleteSource = async (sourceId: string) => {
    if (!confirm("Are you sure you want to remove this source?")) {
      return;
    }

    try {
      const response = await fetch(`${process.env.NEXT_PUBLIC_API_URL}/sources/${encodeURIComponent(sourceId)}`, {
        method: "DELETE",
      });

      if (!response.ok) throw new Error("Failed to delete source");
      
      setSources(sources.filter(source => source.id !== sourceId));
    } catch (err) {
      alert(`Failed to delete source: ${err instanceof Error ? err.message : "Unknown error"}`);
    }
  };

  if (loading) {
    return (
      <div className="loading-container">
        <div className="loading-text">Loading sources...</div>
      </div>
    );
  }

  if (error) {
    return (
      <div className="error-container">
        <div className="error-text">Error: {error}</div>
      </div>
    );
  }

  return (
    <div className="flex-col-full">
      <div className="panel flex-between">
        <h2 className="heading-section">Sources</h2>
        <div className="flex gap-2">
          { hasFeature("duckdb") &&
            <button 
              onClick={() => setShowAddModal(true)}
              className="btn-primary"
            >
              Add Source
            </button>
          }
          <button
            onClick={loadSources}
            className="btn-secondary"
          >
            Refresh
          </button>
        </div>
      </div>

      <div className="flex-1 overflow-y-auto p-6">
        {sources.length === 0 ? (
          <div className="empty-state">
            No sources configured
          </div>
        ) : (
          <div className="space-y-3">
            {sources.map((source) => (
              <div
                key={source.id}
                className="card-bordered"
              >
                <div className="flex flex-col gap-3">
                  {/* Header with type and modify/remove button */}
                  <div className="flex-between">
                    <div className="text-xs font-medium text-gray-500 uppercase tracking-wide">
                      {source.sourcetype ? sourceTypeLabels[source.sourcetype] || source.sourcetype : "Unknown Type"}
                    </div>
                    <button
                      onClick={() => deleteSource(source.id)}
                      className="text-xs text-red-600 hover:text-red-700 font-medium"
                    >
                      Remove
                    </button>
                  </div>

                  {/* Name and toggle */}
                  <div className="flex-between">
                    <div className="flex-1">
                      <div className="font-medium text-gray-900">{source.name}</div>
                      <div className="text-sm text-gray-500 font-mono mt-1">{source.id}</div>
                    </div>
                    <div className="ml-4">
                      <label className="relative inline-flex items-center cursor-pointer">
                        <input
                          type="checkbox"
                          className="sr-only peer"
                          checked={source.enabled}
                          onChange={(e) => toggleSource(source.id, e.target.checked)}
                        />
                        <div className="toggle-switch"></div>
                      </label>
                    </div>
                  </div>
                </div>
              </div>
            ))}
          </div>
        )}
      </div>

      <AddSource
        isOpen={showAddModal}
        onClose={() => setShowAddModal(false)}
        onSourceAdded={loadSources}
      />
    </div>
  );
}
