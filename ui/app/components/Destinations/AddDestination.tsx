"use client";

import { useState } from "react";

interface DestinationType {
  id: string;
  name: string;
  description: string;
}

interface AddDestinationModalProps {
  isOpen: boolean;
  onClose: () => void;
  onDestinationAdded: () => void;
}

const destinationTypes: DestinationType[] = [
  {
    id: "local_file",
    name: "Local Files (parquet)",
    description: "Write OCSF events to local parquet files (also queried by Explore)",
  },
  {
    id: "aws_s3",
    name: "AWS S3",
    description: "Archive OCSF events to an S3 bucket",
  },
  {
    id: "clickhouse",
    name: "Clickhouse",
    description: "Stream OCSF events into a Clickhouse table",
  },
];

export default function AddDestination({ isOpen, onClose, onDestinationAdded }: AddDestinationModalProps) {
  const [step, setStep] = useState<"select" | "configure">("select");
  const [selectedType, setSelectedType] = useState<string>("");
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const [localFile, setLocalFile] = useState({ path: "/data/storage" });
  const [s3, setS3] = useState({
    bucket: "",
    region: "us-east-1",
    key_prefix: "",
    access_key_id: "",
    secret_access_key: "",
  });
  const [clickhouse, setClickhouse] = useState({
    endpoint: "",
    database: "",
    user: "",
    password: "",
  });

  const resetModal = () => {
    setStep("select");
    setSelectedType("");
    setError(null);
    setLoading(false);
    setLocalFile({ path: "/data/storage" });
    setS3({ bucket: "", region: "us-east-1", key_prefix: "", access_key_id: "", secret_access_key: "" });
    setClickhouse({ endpoint: "", database: "", user: "", password: "" });
  };

  const handleClose = () => {
    resetModal();
    onClose();
  };

  const handleNext = () => {
    if (!selectedType) {
      setError("Please select a destination type");
      return;
    }
    setError(null);
    setStep("configure");
  };

  const dropEmpty = (obj: Record<string, string>) =>
    Object.fromEntries(Object.entries(obj).filter(([, v]) => v.trim() !== ""));

  const handleSubmit = async () => {
    try {
      setLoading(true);
      setError(null);

      let config: Record<string, string>;
      if (selectedType === "local_file") {
        if (!localFile.path.trim()) throw new Error("Path is required");
        config = { path: localFile.path.trim() };
      } else if (selectedType === "aws_s3") {
        if (!s3.bucket.trim()) throw new Error("Bucket is required");
        config = dropEmpty(s3);
      } else if (selectedType === "clickhouse") {
        if (!clickhouse.endpoint.trim()) {
          throw new Error("Endpoint is required");
        }
        config = dropEmpty(clickhouse);
      } else {
        throw new Error("Invalid destination type");
      }

      const response = await fetch(`${process.env.NEXT_PUBLIC_API_URL}/destinations/${selectedType}`, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify(config),
      });
      if (!response.ok) {
        const errorText = await response.text();
        throw new Error(`Failed to create destination: ${response.status} ${response.statusText} - ${errorText}`);
      }

      onDestinationAdded();
      handleClose();
    } catch (err) {
      setError(err instanceof Error ? err.message : "Unknown error");
    } finally {
      setLoading(false);
    }
  };

  if (!isOpen) return null;

  return (
    <div className="modal-overlay">
      <div className="bg-white rounded-lg shadow-xl max-w-2xl w-full mx-4 max-h-[90vh] overflow-hidden">
        <div className="p-6">
          <div className="flex justify-between items-center mb-6">
            <h2 className="text-2xl font-semibold text-gray-900">
              {step === "select"
                ? "Add New Destination"
                : `Configure ${destinationTypes.find((d) => d.id === selectedType)?.name}`}
            </h2>
            <button onClick={handleClose} className="text-gray-400 hover:text-gray-600 text-2xl">
              X
            </button>
          </div>

          {error && (
            <div className="mb-4 p-3 bg-red-50 border border-red-200 rounded-md text-red-700">{error}</div>
          )}

          {step === "select" && (
            <div>
              <p className="text-gray-600 mb-6">Select where StrIEM should send normalized OCSF events:</p>
              <div className="space-y-3">
                {destinationTypes.map((type) => (
                  <label
                    key={type.id}
                    className="flex items-start p-4 border border-gray-200 rounded-lg cursor-pointer hover:bg-gray-50"
                  >
                    <input
                      type="radio"
                      name="destinationType"
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

          {step === "configure" && selectedType === "local_file" && (
            <div className="space-y-4">
              <div>
                <label className="input-label">Directory Path *</label>
                <input
                  type="text"
                  value={localFile.path}
                  onChange={(e) => setLocalFile({ path: e.target.value })}
                  className="input-base"
                  placeholder="/data/storage"
                  required
                />
                <p className="text-sm text-gray-500 mt-2">
                  OCSF events are written here as parquet, and the Explore tab queries this path.
                </p>
              </div>
            </div>
          )}

          {step === "configure" && selectedType === "aws_s3" && (
            <div className="space-y-4">
              <div>
                <label className="input-label">Bucket *</label>
                <input
                  type="text"
                  value={s3.bucket}
                  onChange={(e) => setS3({ ...s3, bucket: e.target.value })}
                  className="input-base"
                  placeholder="my-ocsf-bucket"
                  required
                />
              </div>
              <div className="grid grid-cols-2 gap-4">
                <div>
                  <label className="input-label">Region *</label>
                  <input
                    type="text"
                    value={s3.region}
                    onChange={(e) => setS3({ ...s3, region: e.target.value })}
                    className="input-base"
                    placeholder="us-east-1"
                  />
                </div>
                <div>
                  <label className="input-label">Key Prefix</label>
                  <input
                    type="text"
                    value={s3.key_prefix}
                    onChange={(e) => setS3({ ...s3, key_prefix: e.target.value })}
                    className="input-base"
                    placeholder="ocsf/"
                  />
                </div>
              </div>
              <div>
                <label className="input-label">Access Key ID (optional — leave empty to use IAM roles)</label>
                <input
                  type="text"
                  value={s3.access_key_id}
                  onChange={(e) => setS3({ ...s3, access_key_id: e.target.value })}
                  className="input-base"
                />
              </div>
              <div>
                <label className="input-label">Secret Access Key (optional)</label>
                <input
                  type="password"
                  value={s3.secret_access_key}
                  onChange={(e) => setS3({ ...s3, secret_access_key: e.target.value })}
                  className="input-base"
                />
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
                  />
                </div>
              </div>
            </div>
          )}

          <div className="flex justify-end space-x-3 pt-6 mt-6 border-t border-gray-200">
            {step === "select" ? (
              <>
                <button onClick={handleClose} className="btn-tertiary">
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
                <button onClick={handleClose} className="btn-tertiary" disabled={loading}>
                  Cancel
                </button>
                <button onClick={handleSubmit} className="btn-primary" disabled={loading}>
                  {loading ? "Adding..." : "Add Destination"}
                </button>
              </>
            )}
          </div>
        </div>
      </div>
    </div>
  );
}
