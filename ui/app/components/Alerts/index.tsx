"use client";

import { useState, useEffect, useCallback } from "react";
import AlertsFilters from "./AlertFilters";
import AlertDetails from "./AlertDetails";
import ActionConfirmModal from "./Actions";

interface Alert {
  id: string;
  time: string;
  severity: string;
  title: string;
  _file?: string;
}

interface Action {
  id: string;
  title: string;
}

export default function AlertsTab() {
  const [alerts, setAlerts] = useState<Alert[]>([]);
  const [filteredAlerts, setFilteredAlerts] = useState<Alert[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [selectedAlert, setSelectedAlert] = useState<Alert | null>(null);
  const [actions, setActions] = useState<Action[]>([]);
  const [actionsLoading, setActionsLoading] = useState(false);
  const [actionConfirm, setActionConfirm] = useState<{
    action: Action;
    alert: Alert;
  } | null>(null);
  const [openDropdown, setOpenDropdown] = useState<string | null>(null);

  // Time range state
  const [timeRange, setTimeRange] = useState({
    start: new Date(Date.now() - 24 * 60 * 60 * 1000).toISOString().slice(0, 16),
    end: new Date().toISOString().slice(0, 16),
  });

  useEffect(() => {
    loadAlerts();
  }, [timeRange]);

  useEffect(() => {
    loadActions();
  }, []);

  const getTimeZone = () => {
    var offset = new Date().getTimezoneOffset(), o = Math.abs(offset);
    return (offset < 0 ? "+" : "-") + ("00" + Math.floor(o / 60)).slice(-2) + ":" + ("00" + (o % 60)).slice(-2);
  }

  const loadAlerts = async () => {
    try {
      setLoading(true);
      setError(null);
      
      const params = new URLSearchParams({
        start: timeRange.start + ":00" + getTimeZone(),
        end: timeRange.end + ":59" + getTimeZone(),
      });
      
      const response = await fetch(`${process.env.NEXT_PUBLIC_API_URL}/alerts?${params}`);
      if (!response.ok) {
        throw new Error(`Failed to fetch alerts: ${response.status} ${response.statusText}`);
      }
      
      const data = await response.json();
      const alertsArray = Array.isArray(data) ? data : [];
      setAlerts(alertsArray);
      setFilteredAlerts(alertsArray);
    } catch (err) {
      console.error("Error loading alerts:", err);
      setError(err instanceof Error ? err.message : "Unknown error");
    } finally {
      setLoading(false);
    }
  };

  const loadActions = async () => {
    try {
      setActionsLoading(true);
      const response = await fetch(`${process.env.NEXT_PUBLIC_API_URL}/actions`);
      if (!response.ok) {
        throw new Error(`Failed to fetch actions: ${response.status}`);
      }
      
      const data = await response.json();
      setActions(Array.isArray(data) ? data : []);
    } catch (err) {
      console.error("Error loading actions:", err);
      // Don't set error state, just log it
    } finally {
      setActionsLoading(false);
    }
  };

  const handleFilteredAlertsChange = useCallback((newFilteredAlerts: Alert[]) => {
    setFilteredAlerts(newFilteredAlerts);
  }, []);

  const showAlertDetails = (alert: Alert) => {
    setSelectedAlert(alert);
  };

  const handleActionSelect = (action: Action, alert: Alert) => {
    setActionConfirm({ action, alert });
    setOpenDropdown(null);
  };

  const executeAction = async () => {
    if (!actionConfirm) return;

    try {
      const response = await fetch(`${process.env.NEXT_PUBLIC_API_URL}/actions/${actionConfirm.action.id}`, {
        method: "POST",
        headers: {
          "Content-Type": "application/json",
        },
        body: JSON.stringify({ alert_id: actionConfirm.alert.id, file: actionConfirm.alert._file }),
      });

      if (!response.ok) {
        throw new Error("Failed to execute action");
      }

      alert(`Action "${actionConfirm.action.title}" executed successfully for alert ${actionConfirm.alert.id}`);
      setActionConfirm(null);
      
      // Optionally reload alerts
      loadAlerts();
    } catch (err) {
      alert(`Failed to execute action: ${err instanceof Error ? err.message : "Unknown error"}`);
    }
  };

  const getSeverityColor = (severity: string) => {
    const lowerSeverity = severity.toLowerCase();
    if (lowerSeverity === 'critical') return 'bg-red-100 text-red-800 border-red-200';
    if (lowerSeverity === 'high') return 'bg-orange-100 text-orange-800 border-orange-200';
    if (lowerSeverity === 'medium') return 'bg-yellow-100 text-yellow-800 border-yellow-200';
    if (lowerSeverity === 'low') return 'bg-blue-100 text-blue-800 border-blue-200';
    if (lowerSeverity === 'info' || lowerSeverity === 'informational') return 'bg-green-100 text-green-800 border-green-200';
    return 'bg-gray-100 text-gray-800 border-gray-200';
  };

  const formatTime = (timeStr: string) => {
    try {
      const date = new Date(timeStr);
      return date.toLocaleString();
    } catch {
      return timeStr;
    }
  };

  if (loading) {
    return (
      <div className="loading-container">
        <div className="loading-text">Loading alerts...</div>
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
    <div className="h-full flex flex-col">
      {/* Header */}
      <div className="bg-white border-b border-gray-200 px-6 py-4">
        <div className="flex justify-between items-start mb-4">
          <div>
            <h2 className="text-2xl font-semibold text-gray-900">Alerts</h2>
            <p className="text-sm text-gray-500 mt-1">
              {filteredAlerts.length === alerts.length 
                ? `${alerts.length} alerts`
                : `${filteredAlerts.length} of ${alerts.length} alerts`
              }
            </p>
          </div>
          <button
            onClick={loadAlerts}
            className="btn-secondary"
          >
            Refresh
          </button>
        </div>

        {/* Time Picker */}
        <div className="flex items-center gap-4">
          <div className="flex items-center gap-2">
            <label className="input-label">Start:</label>
            <input
              type="datetime-local"
              value={timeRange.start}
              onChange={(e) => setTimeRange(prev => ({...prev, start: e.target.value }))}
              className="input-base"
            />
          </div>
          <div className="flex items-center gap-2">
            <label className="input-label">End:</label>
            <input
              type="datetime-local"
              value={timeRange.end}
              onChange={(e) => setTimeRange(prev => ({ ...prev, end: e.target.value }))}
              className="input-base"
            />
          </div>
          <button
            onClick={() => setTimeRange({
              start: new Date(Date.now() - 24 * 60 * 60 * 1000).toISOString(),//.slice(0, 16),
              end: new Date().toISOString(),//.slice(0, 16),
            })}
            className="text-link-sm"
          >
            Last 24h
          </button>
          <button
            onClick={() => setTimeRange({
              start: new Date(Date.now() - 7 * 24 * 60 * 60 * 1000).toISOString(),//.slice(0, 16),
              end: new Date().toISOString(),//.slice(0, 16),
            })}
            className="text-link-sm"
          >
            Last 7 days
          </button>
        </div>
      </div>

      {/* Filters */}
      <AlertsFilters 
        alerts={alerts}
        onFilteredAlertsChange={handleFilteredAlertsChange}
      />

      {/* Alerts Table */}
      <div className="flex-1 overflow-auto p-6">
        {filteredAlerts.length === 0 ? (
          <div className="text-center text-gray-500 py-12">
            {alerts.length === 0 ? "No alerts found" : "No alerts match the current filters"}
          </div>
        ) : (
          <div className="bg-white rounded-lg shadow overflow-hidden">
            <table className="min-w-full divide-y divide-gray-200">
              <thead className="bg-gray-50">
                <tr>
                  <th className="px-6 py-3 text-left text-xs font-medium text-gray-500 uppercase tracking-wider">
                    Time
                  </th>
                  <th className="px-6 py-3 text-left text-xs font-medium text-gray-500 uppercase tracking-wider">
                    Severity
                  </th>
                  <th className="px-6 py-3 text-left text-xs font-medium text-gray-500 uppercase tracking-wider">
                    Title
                  </th>
                  <th className="px-6 py-3 text-left text-xs font-medium text-gray-500 uppercase tracking-wider">
                    ID
                  </th>
                  <th className="px-6 py-3 text-right text-xs font-medium text-gray-500 uppercase tracking-wider">
                    Actions
                  </th>
                </tr>
              </thead>
              <tbody className="bg-white divide-y divide-gray-200">
                {filteredAlerts.map((alert) => (
                  <tr key={alert.id} className="hover:bg-gray-50">
                    <td className="px-6 py-4 whitespace-nowrap text-sm text-gray-900">
                      {formatTime(alert.time)}
                    </td>
                    <td className="px-6 py-4 whitespace-nowrap">
                      <span className={`inline-flex px-3 py-1 text-xs font-medium rounded-full border ${getSeverityColor(alert.severity)}`}>
                        {alert.severity}
                      </span>
                    </td>
                    <td className="px-6 py-4 text-sm text-gray-900">
                      <button
                        onClick={() => showAlertDetails(alert)}
                        className="text-link"
                      >
                        {alert.title}
                      </button>
                    </td>
                    <td className="px-6 py-4 whitespace-nowrap text-sm text-gray-500 font-mono">
                      {alert.id}
                    </td>
                    <td className="px-6 py-4 whitespace-nowrap text-right text-sm">
                      <div className="relative inline-block">
                        <button
                          onClick={() => setOpenDropdown(openDropdown === alert.id ? null : alert.id)}
                          disabled={actionsLoading || actions.length === 0}
                          className="btn-small"
                        >
                          Actions ▾
                        </button>
                        
                        {openDropdown === alert.id && actions.length > 0 && (
                          <div className="dropdown-menu">
                            {actions.map((action) => (
                              <button
                                key={action.id}
                                onClick={() => handleActionSelect(action, alert)}
                                className="dropdown-item"
                              >
                                {action.title}
                              </button>
                            ))}
                          </div>
                        )}
                      </div>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
      </div>

      {/* Alert Details Modal */}
      {selectedAlert && (
        <AlertDetails
          alert={selectedAlert}
          onClose={() => setSelectedAlert(null)}
        />
      )}

      {/* Action Confirmation Modal */}
      {actionConfirm && (
        <ActionConfirmModal
          action={actionConfirm.action}
          alert={actionConfirm.alert}
          onConfirm={executeAction}
          onCancel={() => setActionConfirm(null)}
        />
      )}
    </div>
  );
}
