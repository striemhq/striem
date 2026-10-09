"use client";

import { useState, useEffect } from "react";
import ConfigureStorage, { StorageInfo, storageTypes } from "./ConfigureStorage";

/** Settings that the list does not show: the `<secret>_set` flags. */
const isSecretFlag = (key: string) => key.endsWith("_set");

export default function StorageTab() {
  const [storage, setStorage] = useState<StorageInfo | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [warning, setWarning] = useState<string | null>(null);
  const [showConfigure, setShowConfigure] = useState(false);

  useEffect(() => {
    loadStorage();
  }, []);

  const loadStorage = async () => {
    try {
      setLoading(true);
      setError(null);
      const response = await fetch(`${process.env.NEXT_PUBLIC_API_URL}/storage`);
      if (!response.ok) {
        const errorText = await response.text();
        throw new Error(`Failed to fetch storage: ${response.status} ${response.statusText} - ${errorText}`);
      }
      setStorage(await response.json());
    } catch (err) {
      setError(err instanceof Error ? err.message : "Unknown error");
    } finally {
      setLoading(false);
    }
  };

  const removeStorage = async () => {
    if (!confirm("Remove this storage? Explore and Alerts will have no data until you configure another one.")) {
      return;
    }
    try {
      const response = await fetch(`${process.env.NEXT_PUBLIC_API_URL}/storage`, { method: "DELETE" });
      if (!response.ok) throw new Error(await response.text());
      setWarning(null);
      setStorage({ configured: false });
    } catch (err) {
      alert(`Failed to remove storage: ${err instanceof Error ? err.message : "Unknown error"}`);
    }
  };

  if (loading) {
    return (
      <div className="loading-container">
        <div className="loading-text">Loading storage...</div>
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

  const configured = storage?.configured ? storage : null;
  const typeLabel = (type?: string) =>
    storageTypes.find((t) => t.id === type)?.name || type || "Unknown Type";

  return (
    <div className="flex-col-full">
      <div className="panel flex-between">
        <h2 className="heading-section">Storage</h2>
        <div className="flex gap-2">
          <button onClick={() => setShowConfigure(true)} className="btn-primary">
            {configured ? "Change Storage" : "Configure Storage"}
          </button>
          <button onClick={loadStorage} className="btn-secondary">
            Refresh
          </button>
        </div>
      </div>

      <div className="flex-1 overflow-y-auto p-6 space-y-4">
        <p className="text-sm text-gray-500">
          The storage is where the Explore and Alerts views read event data. Destinations write the
          events; the storage reads them back.
        </p>

        {warning && (
          <div className="p-3 bg-yellow-50 border border-yellow-200 rounded-md text-yellow-800 text-sm">
            Saved, with a warning: {warning}
          </div>
        )}

        {!configured ? (
          <div className="empty-state">No storage configured. Explore and Alerts have no data.</div>
        ) : (
          <div className="card-bordered">
            <div className="flex flex-col gap-3">
              <div className="flex-between">
                <div className="flex items-center gap-2">
                  <div className="text-xs font-medium text-gray-500 uppercase tracking-wide">
                    {typeLabel(configured.storagetype)}
                  </div>
                  {configured.implemented === false && (
                    <span className="text-xs px-2 py-0.5 rounded-full bg-yellow-100 text-yellow-800 border border-yellow-200">
                      Not implemented yet
                    </span>
                  )}
                </div>
                <button onClick={removeStorage} className="text-xs text-red-600 hover:text-red-700 font-medium">
                  Remove
                </button>
              </div>
              <div>
                <div className="font-medium text-gray-900">{configured.name}</div>
                <div className="text-sm text-gray-500 font-mono mt-1">{configured.id}</div>
              </div>
              {configured.config && (
                <dl className="grid grid-cols-[max-content_1fr] gap-x-4 gap-y-1 text-sm">
                  {Object.entries(configured.config)
                    .filter(([key]) => !isSecretFlag(key))
                    .map(([key, value]) => (
                      <div key={key} className="contents">
                        <dt className="text-gray-500">{key}</dt>
                        <dd className="text-gray-900 font-mono break-all">{String(value)}</dd>
                      </div>
                    ))}
                  {Object.entries(configured.config)
                    .filter(([key]) => isSecretFlag(key))
                    .map(([key, value]) => (
                      <div key={key} className="contents">
                        <dt className="text-gray-500">{key.replace(/_set$/, "")}</dt>
                        <dd className="text-gray-900">{value ? "set" : "not set"}</dd>
                      </div>
                    ))}
                </dl>
              )}
            </div>
          </div>
        )}
      </div>

      <ConfigureStorage
        isOpen={showConfigure}
        current={configured}
        onClose={() => setShowConfigure(false)}
        onSaved={(saved) => {
          setStorage(saved);
          setWarning(saved.warning || null);
        }}
      />
    </div>
  );
}
