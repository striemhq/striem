"use client";

import { useState, useEffect } from "react";

interface Alert {
  id: string;
  time: string;
  severity: string;
  title: string;
}

interface FilterOptions {
  severities: string[];
}

interface Filters {
  severities: string[];
  search: string;
}

interface AlertsFiltersProps {
  alerts: Alert[];
  onFilteredAlertsChange: (filteredAlerts: Alert[]) => void;
}

export default function AlertsFilters({ alerts, onFilteredAlertsChange }: AlertsFiltersProps) {
  const [filters, setFilters] = useState<Filters>({
    severities: [],
    search: ''
  });

  const [filterOptions, setFilterOptions] = useState<FilterOptions>({
    severities: []
  });

  const [isExpanded, setIsExpanded] = useState(false);

  // Extract unique filter options from alerts
  useEffect(() => {
    const severities = [...new Set(alerts.map(alert => alert.severity).filter((sev): sev is string => Boolean(sev)))].sort();

    setFilterOptions({
      severities
    });
  }, [alerts]);

  // Apply filters whenever filters change or alerts change
  useEffect(() => {
    const filteredAlerts = alerts.filter(alert => {
      // Severity filter (multiple severities allowed)
      if (filters.severities.length > 0 && alert.severity && !filters.severities.includes(alert.severity)) {
        return false;
      }

      // Search filter (title, id)
      if (filters.search) {
        const searchLower = filters.search.toLowerCase();
        const matchesTitle = alert.title?.toLowerCase().includes(searchLower);
        const matchesId = alert.id?.toLowerCase().includes(searchLower);
        
        if (!matchesTitle && !matchesId) {
          return false;
        }
      }

      return true;
    });

    onFilteredAlertsChange(filteredAlerts);
  }, [filters, alerts, onFilteredAlertsChange]);

  const updateFilter = (key: keyof Filters, value: string) => {
    setFilters(prev => ({
      ...prev,
      [key]: value
    }));
  };

  const toggleSeverity = (severity: string) => {
    setFilters(prev => ({
      ...prev,
      severities: prev.severities.includes(severity)
        ? prev.severities.filter(s => s !== severity)
        : [...prev.severities, severity]
    }));
  };

  const clearFilters = () => {
    setFilters({
      severities: [],
      search: ''
    });
  };

  const hasActiveFilters = filters.severities.length > 0 || filters.search !== '';
  const activeFilterCount = [
    filters.severities.length > 0,
    filters.search !== ''
  ].filter(Boolean).length;

  return (
    <div className="bg-white border-b border-gray-200">
      <div className="px-6 py-4">
        <div className="flex items-center justify-between">
          <div className="flex items-center space-x-4 flex-1">
            <button
              onClick={() => setIsExpanded(!isExpanded)}
              className="flex items-center space-x-2 text-gray-700 hover:text-gray-900"
            >
              <svg
                className={`w-5 h-5 transform transition-transform ${isExpanded ? 'rotate-90' : ''}`}
                fill="none"
                stroke="currentColor"
                viewBox="0 0 24 24"
              >
                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M9 5l7 7-7 7" />
              </svg>
              <span className="font-medium">Filters</span>
              {activeFilterCount > 0 && (
                <span className="bg-blue-100 text-blue-800 text-xs font-medium px-2 py-1 rounded-full">
                  {activeFilterCount}
                </span>
              )}
            </button>

            {/* Search bar always visible */}
            <div className="flex-1 max-w-md">
              <div className="relative">
                <svg
                  className="absolute left-3 top-1/2 transform -translate-y-1/2 text-gray-400 w-4 h-4"
                  fill="none"
                  stroke="currentColor"
                  viewBox="0 0 24 24"
                >
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M21 21l-6-6m2-5a7 7 0 11-14 0 7 7 0 0114 0z" />
                </svg>
                <input
                  type="text"
                  placeholder="Search alerts..."
                  value={filters.search}
                  onChange={(e) => updateFilter('search', e.target.value)}
                  className="input-base"
                />
              </div>
            </div>
          </div>

          {hasActiveFilters && (
            <button
              onClick={clearFilters}
              className="text-sm text-gray-500 hover:text-gray-700"
            >
              Clear all
            </button>
          )}
        </div>

        {/* Expanded filter controls */}
        {isExpanded && (
          <div className="mt-4">
            <div>
              <div className="flex items-center gap-2 mb-1">
                <label className="block text-sm font-medium text-gray-700">Severity</label>
                {filters.severities.length > 0 && (
                  <span className="bg-blue-100 text-blue-800 text-xs font-medium px-2 py-0.5 rounded">
                    {filters.severities.length}
                  </span>
                )}
              </div>
              <div className="flex items-center justify-between mb-2">
                <p className="text-xs text-gray-500">Click to select multiple severities</p>
                {filters.severities.length > 0 && (
                  <button
                    onClick={() => setFilters(prev => ({ ...prev, severities: [] }))}
                    className="text-xs text-blue-600 hover:text-blue-800"
                  >
                    Clear severities ({filters.severities.length})
                  </button>
                )}
              </div>
              <div className="flex flex-wrap gap-2">
                {filterOptions.severities.map(severity => {
                  const isSelected = filters.severities.includes(severity);
                  let severityColor = 'bg-gray-100 text-gray-800 border-gray-200';
                  
                  if (isSelected) {
                    switch (severity.toLowerCase()) {
                      case 'critical':
                        severityColor = 'bg-red-500 text-white border-red-500';
                        break;
                      case 'high':
                        severityColor = 'bg-orange-500 text-white border-orange-500';
                        break;
                      case 'medium':
                        severityColor = 'bg-yellow-500 text-white border-yellow-500';
                        break;
                      case 'low':
                        severityColor = 'bg-blue-500 text-white border-blue-500';
                        break;
                      case 'info':
                      case 'informational':
                        severityColor = 'bg-green-500 text-white border-green-500';
                        break;
                      default:
                        severityColor = 'bg-gray-500 text-white border-gray-500';
                    }
                  } else {
                    switch (severity.toLowerCase()) {
                      case 'critical':
                        severityColor = 'bg-red-50 text-red-700 border-red-200 hover:bg-red-100';
                        break;
                      case 'high':
                        severityColor = 'bg-orange-50 text-orange-700 border-orange-200 hover:bg-orange-100';
                        break;
                      case 'medium':
                        severityColor = 'bg-yellow-50 text-yellow-700 border-yellow-200 hover:bg-yellow-100';
                        break;
                      case 'low':
                        severityColor = 'bg-blue-50 text-blue-700 border-blue-200 hover:bg-blue-100';
                        break;
                      case 'info':
                      case 'informational':
                        severityColor = 'bg-green-50 text-green-700 border-green-200 hover:bg-green-100';
                        break;
                      default:
                        severityColor = 'bg-gray-50 text-gray-700 border-gray-200 hover:bg-gray-100';
                    }
                  }

                  return (
                    <button
                      key={severity}
                      onClick={() => toggleSeverity(severity)}
                      className={`px-3 py-1 text-xs font-medium rounded-full border-2 transition-colors ${severityColor} ${
                        isSelected ? 'ring-2 ring-offset-1 ring-gray-400' : ''
                      }`}
                      title={isSelected ? `Remove ${severity} filter` : `Add ${severity} filter`}
                    >
                      {isSelected && '✓ '}{severity}
                    </button>
                  );
                })}
              </div>
            </div>
          </div>
        )}
      </div>
    </div>
  );
}
