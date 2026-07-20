"use client";

import { useState } from "react";

interface NotificationType {
  id: string;
  name: string;
  description: string;
}

interface AddNotificationModalProps {
  isOpen: boolean;
  onClose: () => void;
  onNotificationAdded: () => void;
}

const notificationTypes: NotificationType[] = [
  {
    id: "slack",
    name: "Slack",
    description: "Post alerts to a Slack channel via chat.postMessage",
  },
  {
    id: "email",
    name: "Email",
    description: "Send alerts through an email-provider HTTP API",
  },
  {
    id: "webhook",
    name: "Webhook",
    description: "POST each alert as JSON to an arbitrary URL",
  },
];

export default function AddNotification({ isOpen, onClose, onNotificationAdded }: AddNotificationModalProps) {
  const [step, setStep] = useState<"select" | "configure">("select");
  const [selectedType, setSelectedType] = useState<string>("");
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const [slack, setSlack] = useState({ token: "", channel: "" });
  const [email, setEmail] = useState({ endpoint: "", to: "", from: "", subject: "StrIEM Alert", token: "" });
  const [webhook, setWebhook] = useState({ url: "", token: "" });

  const resetModal = () => {
    setStep("select");
    setSelectedType("");
    setError(null);
    setLoading(false);
    setSlack({ token: "", channel: "" });
    setEmail({ endpoint: "", to: "", from: "", subject: "StrIEM Alert", token: "" });
    setWebhook({ url: "", token: "" });
  };

  const handleClose = () => {
    resetModal();
    onClose();
  };

  const handleNext = () => {
    if (!selectedType) {
      setError("Please select a notification type");
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
      if (selectedType === "slack") {
        if (!slack.token.trim() || !slack.channel.trim()) throw new Error("Token and channel are required");
        config = { token: slack.token.trim(), channel: slack.channel.trim() };
      } else if (selectedType === "email") {
        if (!email.endpoint.trim() || !email.to.trim() || !email.from.trim()) {
          throw new Error("Endpoint, to and from are required");
        }
        config = dropEmpty(email);
      } else if (selectedType === "webhook") {
        if (!webhook.url.trim()) throw new Error("URL is required");
        config = dropEmpty(webhook);
      } else {
        throw new Error("Invalid notification type");
      }

      const response = await fetch(`${process.env.NEXT_PUBLIC_API_URL}/notifications/${selectedType}`, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify(config),
      });
      if (!response.ok) {
        const errorText = await response.text();
        throw new Error(`Failed to create notification: ${response.status} ${response.statusText} - ${errorText}`);
      }

      onNotificationAdded();
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
                ? "Add New Notification"
                : `Configure ${notificationTypes.find((n) => n.id === selectedType)?.name}`}
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
              <p className="text-gray-600 mb-6">Select where StrIEM should deliver alerts:</p>
              <div className="space-y-3">
                {notificationTypes.map((type) => (
                  <label
                    key={type.id}
                    className="flex items-start p-4 border border-gray-200 rounded-lg cursor-pointer hover:bg-gray-50"
                  >
                    <input
                      type="radio"
                      name="notificationType"
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

          {step === "configure" && selectedType === "slack" && (
            <div className="space-y-4">
              <div>
                <label className="input-label">Bot Token *</label>
                <input
                  type="password"
                  value={slack.token}
                  onChange={(e) => setSlack({ ...slack, token: e.target.value })}
                  className="input-base"
                  placeholder="xoxb-..."
                  required
                />
              </div>
              <div>
                <label className="input-label">Channel *</label>
                <input
                  type="text"
                  value={slack.channel}
                  onChange={(e) => setSlack({ ...slack, channel: e.target.value })}
                  className="input-base"
                  placeholder="#security-alerts"
                  required
                />
              </div>
            </div>
          )}

          {step === "configure" && selectedType === "email" && (
            <div className="space-y-4">
              <div>
                <label className="input-label">Provider API Endpoint *</label>
                <input
                  type="text"
                  value={email.endpoint}
                  onChange={(e) => setEmail({ ...email, endpoint: e.target.value })}
                  className="input-base"
                  placeholder="https://api.sendgrid.com/v3/mail/send"
                  required
                />
              </div>
              <div className="grid grid-cols-2 gap-4">
                <div>
                  <label className="input-label">To *</label>
                  <input
                    type="email"
                    value={email.to}
                    onChange={(e) => setEmail({ ...email, to: e.target.value })}
                    className="input-base"
                    placeholder="soc@example.com"
                    required
                  />
                </div>
                <div>
                  <label className="input-label">From *</label>
                  <input
                    type="email"
                    value={email.from}
                    onChange={(e) => setEmail({ ...email, from: e.target.value })}
                    className="input-base"
                    placeholder="striem@example.com"
                    required
                  />
                </div>
              </div>
              <div>
                <label className="input-label">Subject</label>
                <input
                  type="text"
                  value={email.subject}
                  onChange={(e) => setEmail({ ...email, subject: e.target.value })}
                  className="input-base"
                />
              </div>
              <div>
                <label className="input-label">API Token (optional)</label>
                <input
                  type="password"
                  value={email.token}
                  onChange={(e) => setEmail({ ...email, token: e.target.value })}
                  className="input-base"
                />
              </div>
            </div>
          )}

          {step === "configure" && selectedType === "webhook" && (
            <div className="space-y-4">
              <div>
                <label className="input-label">URL *</label>
                <input
                  type="text"
                  value={webhook.url}
                  onChange={(e) => setWebhook({ ...webhook, url: e.target.value })}
                  className="input-base"
                  placeholder="https://hooks.example.com/alerts"
                  required
                />
              </div>
              <div>
                <label className="input-label">Bearer Token (optional)</label>
                <input
                  type="password"
                  value={webhook.token}
                  onChange={(e) => setWebhook({ ...webhook, token: e.target.value })}
                  className="input-base"
                />
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
                  {loading ? "Adding..." : "Add Notification"}
                </button>
              </>
            )}
          </div>
        </div>
      </div>
    </div>
  );
}
