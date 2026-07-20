"use client";

import { useState, useEffect } from "react";
import AddDestination from "./AddDestination";

interface DestinationConfig {
  id: string;
  name: string;
  sinktype?: string;
}

const destinationTypeLabels: Record<string, string> = {
  local_file: "Local Files (parquet)",
  aws_s3: "AWS S3",
  clickhouse: "Clickhouse",
};

export default function DestinationsTab() {
  const [destinations, setDestinations] = useState<DestinationConfig[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [showAddModal, setShowAddModal] = useState(false);

  useEffect(() => {
    loadDestinations();
  }, []);

  const loadDestinations = async () => {
    try {
      setLoading(true);
      setError(null);
      const response = await fetch(`${process.env.NEXT_PUBLIC_API_URL}/destinations`);
      if (!response.ok) {
        const errorText = await response.text();
        throw new Error(`Failed to fetch destinations: ${response.status} ${response.statusText} - ${errorText}`);
      }
      const data = await response.json();
      if (!data || !Array.isArray(data)) {
        throw new Error("Unexpected destinations response");
      }
      setDestinations(data);
    } catch (err) {
      setError(err instanceof Error ? err.message : "Unknown error");
    } finally {
      setLoading(false);
    }
  };

  const deleteDestination = async (id: string) => {
    if (!confirm("Are you sure you want to remove this destination?")) {
      return;
    }
    try {
      const response = await fetch(`${process.env.NEXT_PUBLIC_API_URL}/destinations/${encodeURIComponent(id)}`, {
        method: "DELETE",
      });
      if (!response.ok) throw new Error("Failed to delete destination");
      setDestinations(destinations.filter((d) => d.id !== id));
    } catch (err) {
      alert(`Failed to delete destination: ${err instanceof Error ? err.message : "Unknown error"}`);
    }
  };

  if (loading) {
    return (
      <div className="loading-container">
        <div className="loading-text">Loading destinations...</div>
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
        <h2 className="heading-section">Destinations</h2>
        <div className="flex gap-2">
          <button onClick={() => setShowAddModal(true)} className="btn-primary">
            Add Destination
          </button>
          <button onClick={loadDestinations} className="btn-secondary">
            Refresh
          </button>
        </div>
      </div>

      <div className="flex-1 overflow-y-auto p-6">
        {destinations.length === 0 ? (
          <div className="empty-state">No destinations configured</div>
        ) : (
          <div className="space-y-3">
            {destinations.map((destination) => (
              <div key={destination.id} className="card-bordered">
                <div className="flex flex-col gap-3">
                  <div className="flex-between">
                    <div className="text-xs font-medium text-gray-500 uppercase tracking-wide">
                      {destination.sinktype
                        ? destinationTypeLabels[destination.sinktype] || destination.sinktype
                        : "Unknown Type"}
                    </div>
                    <button
                      onClick={() => deleteDestination(destination.id)}
                      className="text-xs text-red-600 hover:text-red-700 font-medium"
                    >
                      Remove
                    </button>
                  </div>
                  <div>
                    <div className="font-medium text-gray-900">{destination.name}</div>
                    <div className="text-sm text-gray-500 font-mono mt-1">{destination.id}</div>
                  </div>
                </div>
              </div>
            ))}
          </div>
        )}
      </div>

      <AddDestination
        isOpen={showAddModal}
        onClose={() => setShowAddModal(false)}
        onDestinationAdded={loadDestinations}
      />
    </div>
  );
}
