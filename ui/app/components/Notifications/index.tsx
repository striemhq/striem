"use client";

import { useState, useEffect } from "react";
import AddNotification from "./AddNotification";

interface NotificationConfig {
  id: string;
  name: string;
  sinktype?: string;
}

const notificationTypeLabels: Record<string, string> = {
  slack: "Slack",
  email: "Email",
  webhook: "Webhook",
};

export default function NotificationsTab() {
  const [notifications, setNotifications] = useState<NotificationConfig[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [showAddModal, setShowAddModal] = useState(false);

  useEffect(() => {
    loadNotifications();
  }, []);

  const loadNotifications = async () => {
    try {
      setLoading(true);
      setError(null);
      const response = await fetch(`${process.env.NEXT_PUBLIC_API_URL}/notifications`);
      if (!response.ok) {
        const errorText = await response.text();
        throw new Error(`Failed to fetch notifications: ${response.status} ${response.statusText} - ${errorText}`);
      }
      const data = await response.json();
      if (!data || !Array.isArray(data)) {
        throw new Error("Unexpected notifications response");
      }
      setNotifications(data);
    } catch (err) {
      setError(err instanceof Error ? err.message : "Unknown error");
    } finally {
      setLoading(false);
    }
  };

  const deleteNotification = async (id: string) => {
    if (!confirm("Are you sure you want to remove this notification?")) {
      return;
    }
    try {
      const response = await fetch(`${process.env.NEXT_PUBLIC_API_URL}/notifications/${encodeURIComponent(id)}`, {
        method: "DELETE",
      });
      if (!response.ok) throw new Error("Failed to delete notification");
      setNotifications(notifications.filter((n) => n.id !== id));
    } catch (err) {
      alert(`Failed to delete notification: ${err instanceof Error ? err.message : "Unknown error"}`);
    }
  };

  if (loading) {
    return (
      <div className="loading-container">
        <div className="loading-text">Loading notifications...</div>
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
        <h2 className="heading-section">Notifications</h2>
        <div className="flex gap-2">
          <button onClick={() => setShowAddModal(true)} className="btn-primary">
            Add Notification
          </button>
          <button onClick={loadNotifications} className="btn-secondary">
            Refresh
          </button>
        </div>
      </div>

      <div className="flex-1 overflow-y-auto p-6">
        {notifications.length === 0 ? (
          <div className="empty-state">No notifications configured</div>
        ) : (
          <div className="space-y-3">
            {notifications.map((notification) => (
              <div key={notification.id} className="card-bordered">
                <div className="flex flex-col gap-3">
                  <div className="flex-between">
                    <div className="text-xs font-medium text-gray-500 uppercase tracking-wide">
                      {notification.sinktype
                        ? notificationTypeLabels[notification.sinktype] || notification.sinktype
                        : "Unknown Type"}
                    </div>
                    <button
                      onClick={() => deleteNotification(notification.id)}
                      className="text-xs text-red-600 hover:text-red-700 font-medium"
                    >
                      Remove
                    </button>
                  </div>
                  <div>
                    <div className="font-medium text-gray-900">{notification.name}</div>
                    <div className="text-sm text-gray-500 font-mono mt-1">{notification.id}</div>
                  </div>
                </div>
              </div>
            ))}
          </div>
        )}
      </div>

      <AddNotification
        isOpen={showAddModal}
        onClose={() => setShowAddModal(false)}
        onNotificationAdded={loadNotifications}
      />
    </div>
  );
}
