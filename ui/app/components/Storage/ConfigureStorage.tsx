"use client";

import { useEffect, useState } from "react";

/** The storage as the API gives it. Secrets are absent: `<key>_set` tells if each has a value. */
export interface StorageInfo {
  configured: boolean;
  id?: string;
  storagetype?: string;
  name?: string;
  implemented?: boolean;
  config?: Record<string, string | boolean | null>;
  warning?: string;
}

interface StorageType {
  id: string;
  name: string;
  description: string;
  implemented: boolean;
}

export const storageTypes: StorageType[] = [
  {
    id: "clickhouse",
    name: "Clickhouse",
    description: "Read events and alerts from the Clickhouse table that a Clickhouse destination writes",
    implemented: true,
  },
  {
    id: "athena",
    name: "AWS Athena",
    description: "Query OCSF tables with AWS Athena (not implemented yet: settings are saved only)",
    implemented: false,
  },
];

interface ConfigureStorageProps {
  isOpen: boolean;
  current: StorageInfo | null;
  onClose: () => void;
  onSaved: (storage: StorageInfo) => void;
}

const emptyClickhouse = { endpoint: "", database: "", table: "", user: "", password: "" };
const emptyAthena = { region: "us-east-1", database: "", workgroup: "", output_location: "" };

/** Gives a string setting from the current config, or the fallback. */
const str = (config: StorageInfo["config"], key: string, fallback = "") => {
  const value = config?.[key];
  return typeof value === "string" ? value : fallback;
};

export default function ConfigureStorage({ isOpen, current, onClose, onSaved }: ConfigureStorageProps) {
  const [step, setStep] = useState<"select" | "configure">("select");
  const [selectedType, setSelectedType] = useState<string>("");
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [clickhouse, setClickhouse] = useState(emptyClickhouse);
  const [athena, setAthena] = useState(emptyAthena);

  // Start from the current settings, so a change only needs the new values.
  useEffect(() => {
    if (!isOpen) return;
    const config = current?.config;
    setStep(current?.storagetype ? "configure" : "select");
    setSelectedType(current?.storagetype || "");
    setError(null);
    setClickhouse(
      current?.storagetype === "clickhouse"
        ? {
            endpoint: str(config, "endpoint"),
            database: str(config, "database"),
            table: str(config, "table"),
            user: str(config, "user"),
            password: "",
          }
        : emptyClickhouse,
    );
    setAthena(
      current?.storagetype === "athena"
        ? {
            region: str(config, "region", "us-east-1"),
            database: str(config, "database"),
            workgroup: str(config, "workgroup"),
            output_location: str(config, "output_location"),
          }
        : emptyAthena,
    );
  }, [isOpen, current]);

  // The password field is empty when the saved password is kept.
  const keepsPassword = current?.storagetype === "clickhouse" && current?.config?.password_set === true;

  const dropEmpty = (obj: Record<string, string>) =>
    Object.fromEntries(Object.entries(obj).filter(([, v]) => v.trim() !== ""));

  const handleNext = () => {
    if (!selectedType) {
      setError("Please select a storage type");
      return;
    }
    setError(null);
    setStep("configure");
  };

  const handleSubmit = async () => {
    try {
      setLoading(true);
      setError(null);

      let config: Record<string, string>;
      if (selectedType === "clickhouse") {
        if (!clickhouse.endpoint.trim()) throw new Error("Endpoint is required");
        config = dropEmpty(clickhouse);
      } else if (selectedType === "athena") {
        if (!athena.region.trim() || !athena.database.trim()) {
          throw new Error("Region and database are required");
        }
        config = dropEmpty(athena);
      } else {
        throw new Error("Invalid storage type");
      }

      const response = await fetch(`${process.env.NEXT_PUBLIC_API_URL}/storage/${selectedType}`, {
        method: "PUT",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify(config),
      });
      if (!response.ok) {
        const errorText = await response.text();
        throw new Error(`Failed to save storage: ${response.status} ${response.statusText} - ${errorText}`);
      }

      onSaved(await response.json());
      onClose();
    } catch (err) {
      setError(err instanceof Error ? err.message : "Unknown error");
    } finally {
      setLoading(false);
    }
  };

  if (!isOpen) return null;

  const selected = storageTypes.find((t) => t.id === selectedType);

  return (
    <div className="modal-overlay">
      <div className="bg-white rounded-lg shadow-xl max-w-2xl w-full mx-4 max-h-[90vh] overflow-hidden">
        <div className="p-6">
          <div className="flex justify-between items-center mb-6">
            <h2 className="text-2xl font-semibold text-gray-900">
              {step === "select" ? "Configure Storage" : `Configure ${selected?.name}`}
            </h2>
            <button onClick={onClose} className="text-gray-400 hover:text-gray-600 text-2xl">
              X
            </button>
          </div>

          {error && <div className="mb-4 p-3 bg-red-50 border border-red-200 rounded-md text-red-700">{error}</div>}

          {step === "select" && (
            <div>
              <p className="text-gray-600 mb-6">Select where Explore and Alerts should read event data:</p>
              <div className="space-y-3">
                {storageTypes.map((type) => (
                  <label
                    key={type.id}
                    className="flex items-start p-4 border border-gray-200 rounded-lg cursor-pointer hover:bg-gray-50"
                  >
                    <input
                      type="radio"
                      name="storageType"
                      value={type.id}
                      checked={selectedType === type.id}
                      onChange={(e) => setSelectedType(e.target.value)}
                      className="input-radio"
                    />
                    <div className="ml-3">
                      <div className="font-medium text-gray-900">{type.name}</div>
                      <div className="text-sm text-gray-500">{type.description}</div>
                    </div>
                  </label>
                ))}
              </div>
            </div>
          )}

          {step === "configure" && selectedType === "clickhouse" && (
            <div className="space-y-4">
              <div>
                <label className="input-label">Endpoint *</label>
                <input
                  type="text"
                  value={clickhouse.endpoint}
                  onChange={(e) => setClickhouse({ ...clickhouse, endpoint: e.target.value })}
                  className="input-base"
                  placeholder="http://clickhouse:8123"
                  required
                />
              </div>
              <div className="grid grid-cols-2 gap-4">
                <div>
                  <label className="input-label">Database (optional)</label>
                  <input
                    type="text"
                    value={clickhouse.database}
                    onChange={(e) => setClickhouse({ ...clickhouse, database: e.target.value })}
                    className="input-base"
                    placeholder="default"
                  />
                </div>
                <div>
                  <label className="input-label">Table (optional)</label>
                  <input
                    type="text"
                    value={clickhouse.table}
                    onChange={(e) => setClickhouse({ ...clickhouse, table: e.target.value })}
                    className="input-base"
                    placeholder="striem_ocsf"
                  />
                </div>
              </div>
              <div className="grid grid-cols-2 gap-4">
                <div>
                  <label className="input-label">User (optional)</label>
                  <input
                    type="text"
                    value={clickhouse.user}
                    onChange={(e) => setClickhouse({ ...clickhouse, user: e.target.value })}
                    className="input-base"
                  />
                </div>
                <div>
                  <label className="input-label">Password (optional)</label>
                  <input
                    type="password"
                    value={clickhouse.password}
                    onChange={(e) => setClickhouse({ ...clickhouse, password: e.target.value })}
                    className="input-base"
                    placeholder={keepsPassword ? "(unchanged)" : ""}
                  />
                </div>
              </div>
              <p className="text-sm text-gray-500">
                Queries run read-only. A Clickhouse user that can only read this database is recommended.
              </p>
            </div>
          )}

          {step === "configure" && selectedType === "athena" && (
            <div className="space-y-4">
              <div className="p-3 bg-yellow-50 border border-yellow-200 rounded-md text-yellow-800 text-sm">
                Athena is not implemented yet. The settings are saved, but Explore and Alerts will report that
                the storage is not available.
              </div>
              <div className="grid grid-cols-2 gap-4">
                <div>
                  <label className="input-label">Region *</label>
                  <input
                    type="text"
                    value={athena.region}
                    onChange={(e) => setAthena({ ...athena, region: e.target.value })}
                    className="input-base"
                    placeholder="us-east-1"
                  />
                </div>
                <div>
                  <label className="input-label">Database *</label>
                  <input
                    type="text"
                    value={athena.database}
                    onChange={(e) => setAthena({ ...athena, database: e.target.value })}
                    className="input-base"
                    placeholder="ocsf"
                  />
                </div>
              </div>
              <div className="grid grid-cols-2 gap-4">
                <div>
                  <label className="input-label">Workgroup (optional)</label>
                  <input
                    type="text"
                    value={athena.workgroup}
                    onChange={(e) => setAthena({ ...athena, workgroup: e.target.value })}
                    className="input-base"
                    placeholder="primary"
                  />
                </div>
                <div>
                  <label className="input-label">Results Location (optional)</label>
                  <input
                    type="text"
                    value={athena.output_location}
                    onChange={(e) => setAthena({ ...athena, output_location: e.target.value })}
                    className="input-base"
                    placeholder="s3://bucket/athena-results/"
                  />
                </div>
              </div>
            </div>
          )}

          <div className="flex justify-end space-x-3 pt-6 mt-6 border-t border-gray-200">
            {step === "select" ? (
              <>
                <button onClick={onClose} className="btn-tertiary">
                  Cancel
                </button>
                <button onClick={handleNext} className="btn-primary" disabled={!selectedType}>
                  Next
                </button>
              </>
            ) : (
              <>
                <button onClick={() => setStep("select")} className="btn-tertiary" disabled={loading}>
                  Back
                </button>
                <button onClick={onClose} className="btn-tertiary" disabled={loading}>
                  Cancel
                </button>
                <button onClick={handleSubmit} className="btn-primary" disabled={loading}>
                  {loading ? "Saving..." : "Save Storage"}
                </button>
              </>
            )}
          </div>
        </div>
      </div>
    </div>
  );
}
