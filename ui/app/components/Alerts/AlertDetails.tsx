"use client";

import { useState, useEffect } from "react";

interface Alert {
  id: string;
  time: string;
  severity: string;
  title: string;
  _file?: string;
}

interface AlertDetails extends Alert {
  [key: string]: any;
}

interface AlertDetailsModalProps {
  alert: Alert;
  onClose: () => void;
}

export default function AlertDetails({ alert, onClose }: AlertDetailsModalProps) {
  const [details, setDetails] = useState<AlertDetails | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    loadAlertDetails();
  }, [alert.id]);

  const loadAlertDetails = async () => {
    try {
      setLoading(true);
      setError(null);
      
      const response = await fetch(`${process.env.NEXT_PUBLIC_API_URL}/alerts/${encodeURIComponent(alert.id)}?f=${alert._file || ''}`);
      if (!response.ok) {
        throw new Error(`Failed to fetch alert details: ${response.status}`);
      }
      
      const data = await response.json();
      setDetails(data);
    } catch (err) {
      console.error("Error loading alert details:", err);
      setError(err instanceof Error ? err.message : "Unknown error");
    } finally {
      setLoading(false);
    }
  };

  const formatValue = (value: any): string => {
    if (value === null || value === undefined) {
      return "null";
    }
    if (typeof value === "object") {
      return JSON.stringify(value, null, 2);
    }
    return String(value);
  };

  const formatTime = (timeStr: string) => {
    try {
      const date = new Date(timeStr);
      return date.toLocaleString();
    } catch {
      return timeStr;
    }
  };

  const getSeverityColor = (severity: string) => {
    const lowerSeverity = severity.toLowerCase();
    if (lowerSeverity === 'critical') return 'bg-red-100 text-red-800';
    if (lowerSeverity === 'high') return 'bg-orange-100 text-orange-800';
    if (lowerSeverity === 'medium') return 'bg-yellow-100 text-yellow-800';
    if (lowerSeverity === 'low') return 'bg-blue-100 text-blue-800';
    if (lowerSeverity === 'info' || lowerSeverity === 'informational') return 'bg-green-100 text-green-800';
    return 'bg-gray-100 text-gray-800';
  };

  return (
    <div className="modal-overlay">
      <div className="modal-content-xl">
        {/* Header */}
        <div className="modal-header">
          <div>
            <h3 className="modal-title">Alert Details</h3>
            <p className="text-sm text-gray-500 mt-1">ID: {alert.id}</p>
          </div>
          <button
            onClick={onClose}
            className="modal-close-btn"
          >
            ×
          </button>
        </div>

        {/* Content */}
        <div className="flex-1 overflow-y-auto p-6">
          {loading ? (
            <div className="flex items-center justify-center py-12">
              <div className="text-gray-600">Loading alert details...</div>
            </div>
          ) : error ? (
            <div className="bg-red-50 border border-red-200 rounded-lg p-4">
              <div className="flex items-start">
                <span className="text-red-400 text-xl mr-3">⚠️</span>
                <div>
                  <h4 className="text-sm font-medium text-red-800 mb-1">Error Loading Details</h4>
                  <p className="text-sm text-red-700">{error}</p>
                </div>
              </div>
            </div>
          ) : details ? (
            <div className="space-y-6">
              {/* Summary Section */}
              <div className="bg-gray-50 rounded-lg p-4 space-y-3">
                <div className="flex items-start justify-between">
                  <div>
                    <h4 className="text-sm font-medium text-gray-500 mb-1">Title</h4>
                    <p className="text-base font-medium text-gray-900">{details.title}</p>
                  </div>
                  <span className={`inline-flex px-3 py-1 text-xs font-medium rounded-full ${getSeverityColor(details.severity)}`}>
                    {details.severity}
                  </span>
                </div>
                
                <div>
                  <h4 className="text-sm font-medium text-gray-500 mb-1">Time</h4>
                  <p className="text-sm text-gray-900">{formatTime(details.time)}</p>
                </div>
              </div>

              {/* All Fields Section */}
              <div>
                <h4 className="text-sm font-medium text-gray-900 mb-3">All Fields</h4>
                <div className="bg-gray-50 rounded-lg border border-gray-200">
                  <table className="min-w-full divide-y divide-gray-200">
                    <thead className="bg-gray-100">
                      <tr>
                        <th className="px-4 py-2 text-left text-xs font-medium text-gray-500 uppercase">
                          Field
                        </th>
                        <th className="px-4 py-2 text-left text-xs font-medium text-gray-500 uppercase">
                          Value
                        </th>
                      </tr>
                    </thead>
                    <tbody className="divide-y divide-gray-200">
                      {Object.entries(details).map(([key, value]) => (
                        <tr key={key} className="hover:bg-white">
                          <td className="px-4 py-2 text-sm font-medium text-gray-700 align-top">
                            {key}
                          </td>
                          <td className="px-4 py-2 text-sm text-gray-900">
                            {typeof value === "object" ? (
                              <pre className="text-xs bg-white p-2 rounded border border-gray-200 overflow-x-auto">
                                {formatValue(value)}
                              </pre>
                            ) : (
                              <span className="break-all">{formatValue(value)}</span>
                            )}
                          </td>
                        </tr>
                      ))}
                    </tbody>
                  </table>
                </div>
              </div>
            </div>
          ) : null}
        </div>

        {/* Footer */}
        <div className="modal-footer">
          <button
            onClick={onClose}
            className="btn-secondary"
          >
            Close
          </button>
        </div>
      </div>
    </div>
  );
}
