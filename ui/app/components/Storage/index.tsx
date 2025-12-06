"use client";

import { useState } from "react";

export default function StorageTab() {
  const [enableParquetStorage, setEnableParquetStorage] = useState(false);
  const [storageLocation, setStorageLocation] = useState("");
  const [enableWebhookForwarding, setEnableWebhookForwarding] = useState(false);
  const [webhookEventType, setWebhookEventType] = useState<"all" | "alerts">("all");
  const [webhookUrl, setWebhookUrl] = useState("");
  const [webhookUrlError, setWebhookUrlError] = useState("");

  const validateWebhookUrl = (url: string) => {
    if (!url) {
      setWebhookUrlError("");
      return false;
    }
    
    try {
      const urlObj = new URL(url);
      if (urlObj.protocol !== "http:" && urlObj.protocol !== "https:") {
        setWebhookUrlError("URL must use HTTP or HTTPS protocol");
        return false;
      }
      setWebhookUrlError("");
      return true;
    } catch {
      setWebhookUrlError("Please enter a valid HTTP URL");
      return false;
    }
  };

  const handleWebhookUrlChange = (url: string) => {
    setWebhookUrl(url);
    validateWebhookUrl(url);
  };

  const handleSave = () => {
    // TODO: Implement save functionality to send to API
    console.log("Storage settings:", {
      enableParquetStorage,
      storageLocation,
      enableWebhookForwarding,
      webhookEventType,
      webhookUrl
    });
  };

  return (
    <div className="container-full">
      <div className="container-padded">
        <div className="container-section">
          <h2 className="heading-page">Storage Configuration</h2>
          <p className="text-description">Configure data storage settings for your Striem instance.</p>
        </div>

        <div className="card max-w-form">
          <div className="space-y-form">
            {/* Enable Parquet Storage */}
            <div className="flex-start gap-4">
              <div className="flex items-center h-6">
                <input
                  type="checkbox"
                  id="enable-parquet"
                  checked={enableParquetStorage}
                  onChange={(e) => setEnableParquetStorage(e.target.checked)}
                  className="checkbox-base"
                />
              </div>
              <div className="flex-1">
                <label htmlFor="enable-parquet" className="label-inline mb-1">
                  Enable Parquet Storage
                </label>
                <p className="text-muted">
                  Store processed events in Apache Parquet format for efficient querying and analytics.
                </p>
              </div>
            </div>

            {/* Storage Location */}
            {enableParquetStorage && (
              <>
                <div>
                  <label htmlFor="storage-location" className="label-base">
                    Storage Location
                  </label>
                  <input
                    type="text"
                    id="storage-location"
                    value={storageLocation}
                    onChange={(e) => setStorageLocation(e.target.value)}
                    placeholder="/data/storage or s3://bucket-name/path"
                    className="input-base"
                  />
                  <p className="text-helper">
                    Specify the local path or S3 bucket URL where data should be stored.
                  </p>
                </div>

                {/* Save Button */}
                <div className="pt-4">
                  <button
                    onClick={handleSave}
                    className="btn-primary"
                  >
                    Save Configuration
                  </button>
                </div>
              </>
            )}

            {/* Divider */}
            <div className="divider"></div>

            {/* Enable Webhook Forwarding */}
            <div className="flex-start gap-4">
              <div className="flex items-center h-6">
                <input
                  type="checkbox"
                  id="enable-webhook"
                  checked={enableWebhookForwarding}
                  onChange={(e) => setEnableWebhookForwarding(e.target.checked)}
                  className="checkbox-base"
                />
              </div>
              <div className="flex-1">
                <label htmlFor="enable-webhook" className="label-inline mb-1">
                  Forward Events to Webhook
                </label>
                <p className="text-muted">
                  Forward events in real-time to an external webhook endpoint for custom processing or integration.
                </p>
              </div>
            </div>

            {/* Webhook Configuration */}
            {enableWebhookForwarding && (
              <>
                <div>
                  <label className="label-base">
                    Event Type
                  </label>
                  <div className="space-y-2">
                    <div className="flex items-center">
                      <input
                        type="radio"
                        id="webhook-all-events"
                        name="webhook-event-type"
                        value="all"
                        checked={webhookEventType === "all"}
                        onChange={(e) => setWebhookEventType(e.target.value as "all")}
                        className="radio-base"
                      />
                      <label htmlFor="webhook-all-events" className="ml-2 text-sm text-gray-700">
                        All events
                      </label>
                    </div>
                    <div className="flex items-center">
                      <input
                        type="radio"
                        id="webhook-alerts-only"
                        name="webhook-event-type"
                        value="alerts"
                        checked={webhookEventType === "alerts"}
                        onChange={(e) => setWebhookEventType(e.target.value as "alerts")}
                        className="radio-base"
                      />
                      <label htmlFor="webhook-alerts-only" className="ml-2 text-sm text-gray-700">
                        Alerts only
                      </label>
                    </div>
                  </div>
                  <p className="text-helper">
                    Choose whether to forward all events or only security alerts.
                  </p>
                </div>

                <div>
                  <label htmlFor="webhook-url" className="label-base">
                    Webhook URL
                  </label>
                  <input
                    type="url"
                    id="webhook-url"
                    value={webhookUrl}
                    onChange={(e) => handleWebhookUrlChange(e.target.value)}
                    placeholder="https://example.com/webhook"
                    className={`input-base ${
                      webhookUrlError 
                        ? "input-error" 
                        : ""
                    }`}
                  />
                  {webhookUrlError ? (
                    <p className="text-error">
                      {webhookUrlError}
                    </p>
                  ) : (
                    <p className="text-helper">
                      Enter the HTTP or HTTPS URL where events will be sent via POST request.
                    </p>
                  )}
                </div>

                {/* Save Button */}
                <div className="pt-4">
                  <button
                    onClick={handleSave}
                    disabled={!!webhookUrlError || !webhookUrl}
                    className="btn-primary btn-disabled"
                  >
                    Save Configuration
                  </button>
                </div>
              </>
            )}
          </div>
        </div>

        {/* Storage Status */}
        <div className="mt-6 panel-section max-w-form">
          <h3 className="heading-card mb-3">Current Status</h3>
          <div className="space-y-2 text-sm">
            <div className="flex-between">
              <span className="text-gray-600">Parquet Storage:</span>
              <span className={enableParquetStorage ? "status-active" : "status-inactive"}>
                {enableParquetStorage ? "Enabled" : "Disabled"}
              </span>
            </div>
            <div className="flex-between">
              <span className="text-gray-600">Storage Location:</span>
              <span className="text-mono-dark">
                {storageLocation || "Not configured"}
              </span>
            </div>
            <div className="flex-between border-t border-gray-200 pt-2 mt-2">
              <span className="text-gray-600">Webhook Forwarding:</span>
              <span className={enableWebhookForwarding ? "status-active" : "status-inactive"}>
                {enableWebhookForwarding ? "Enabled" : "Disabled"}
              </span>
            </div>
            {enableWebhookForwarding && (
              <>
                <div className="flex-between">
                  <span className="text-gray-600">Event Type:</span>
                  <span className="text-gray-900 font-medium">
                    {webhookEventType === "all" ? "All events" : "Alerts only"}
                  </span>
                </div>
                <div className="flex-between">
                  <span className="text-gray-600">Webhook URL:</span>
                  <span className="text-mono-dark break-word">
                    {webhookUrl || "Not configured"}
                  </span>
                </div>
              </>
            )}
          </div>
        </div>
      </div>
    </div>
  );
}
