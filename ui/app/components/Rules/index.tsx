"use client";

import { useState, useEffect, useCallback } from "react";
import RulesFilters from "./RulesFilters";
import { SigmaRule } from "@types";

export default function RulesTab() {
  const [rules, setRules] = useState<SigmaRule[]>([]);
  const [filteredRules, setFilteredRules] = useState<SigmaRule[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [selectedRule, setSelectedRule] = useState<any>(null);
  const [sourceColors, setSourceColors] = useState<Record<string, string>>({});

  useEffect(() => {
    loadRules();
  }, []);

  const getColour = async (input: string): Promise<string> => {
    const hashBuffer = await window.crypto.subtle.digest("SHA-1", new TextEncoder().encode(input));

    // ofsetting the hash just makes `aws cloudtrail` & `github audit` look nicer...
    const hashArray = Array.from(new Uint8Array(hashBuffer)).slice(2, 5);
    const hashHex = hashArray.map(b => b.toString(16).padStart(2, '0')).join('');
    return `#${hashHex}`;
  }

  // Generate colors for sources when rules change
  useEffect(() => {
    const generateColors = async () => {
      const colors: Record<string, string> = {};
      const sources = new Set<string>();
      
      rules.forEach(rule => {
        if (rule.logsource?.product) sources.add(rule.logsource.product);
        if (rule.logsource?.service) sources.add(rule.logsource.service);
      });
      
      for (const source of sources) {
        colors[source] = await getColour(source);
      }
      
      setSourceColors(colors);
    };
    
    if (rules.length > 0) {
      generateColors();
    }
  }, [rules]);

  const loadRules = async () => {
    try {
      setLoading(true);
      setError(null);
      const response = await fetch(`${process.env.NEXT_PUBLIC_API_URL}/detections`);
      if (!response.ok) {
        const errorText = await response.text();
        throw new Error(`Failed to fetch rules: ${response.status} ${response.statusText} - ${errorText}`);
      }
      
      const data = await response.json();
      console.log("API Response:", data); // Debug log
      if (!data || !Array.isArray(data)) {
        throw new Error("API response does not contain rules array");
      }
      setRules(data);
      setFilteredRules(data); // Initialize filtered rules
    } catch (err) {
      console.error("Error loading rules:", err); // Debug log
      setError(err instanceof Error ? err.message : "Unknown error");
    } finally {
      setLoading(false);
    }
  };

  const handleFilteredRulesChange = useCallback((newFilteredRules: SigmaRule[]) => {
    setFilteredRules(newFilteredRules);
  }, []);

  const toggleRule = async (ruleId: string, enabled: boolean) => {
    try {
      const response = await fetch(`${process.env.NEXT_PUBLIC_API_URL}/detections/${encodeURIComponent(ruleId)}`, {
        method: "PATCH",
        headers: {
          "Content-Type": "application/json",
        },
        body: JSON.stringify({ "enabled":enabled }),
      });

      if (!response.ok) throw new Error("Failed to toggle rule");
      
      const updatedRules = rules.map(rule => 
        rule.id === ruleId ? { ...rule, enabled } : rule
      );
      setRules(updatedRules);
      // Update filtered rules to reflect the change
      setFilteredRules(prev => prev.map(rule => 
        rule.id === ruleId ? { ...rule, enabled } : rule
      ));
    } catch (err) {
      alert(`Failed to toggle rule: ${err instanceof Error ? err.message : "Unknown error"}`);
    }
  };

  const showRuleDetails = async (ruleId: string) => {
    try {
      const response = await fetch(`${process.env.NEXT_PUBLIC_API_URL}/detections/${encodeURIComponent(ruleId)}`);
      if (!response.ok) throw new Error("Failed to fetch rule details");
      
      const rule = await response.json();
      setSelectedRule(rule);
    } catch (err) {
      alert(`Failed to load rule details: ${err instanceof Error ? err.message : "Unknown error"}`);
    }
  };

  const handleFileUpload = async (event: React.ChangeEvent<HTMLInputElement>) => {
    const file = event.target.files?.[0];
    if (!file) return;

    // Validate file extension
    if (!file.name.endsWith('.yaml') && !file.name.endsWith('.yml')) {
      alert('Please upload a YAML file (.yaml or .yml)');
      return;
    }

    try {
      const text = await file.text();
      const response = await fetch(`${process.env.NEXT_PUBLIC_API_URL}/detections`, {
        method: 'POST',
        headers: {
          'Content-Type': 'application/x-yaml',
        },
        body: text,
      });

      if (!response.ok) {
        const errorText = await response.text();
        throw new Error(errorText || 'Failed to upload rule');
      }

      alert('Rule uploaded successfully!');
      loadRules(); // Reload rules
    } catch (err) {
      alert(`Failed to upload rule: ${err instanceof Error ? err.message : 'Unknown error'}`);
    } finally {
      // Reset input so same file can be uploaded again
      event.target.value = '';
    }
  };

  if (loading) {
    return (
      <div className="loading-container">
        <div className="loading-text">Loading rules...</div>
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
      <div className="bg-white border-b border-gray-200 px-6 py-4 flex justify-between items-center">
        <div>
          <h2 className="text-2xl font-semibold text-gray-900">Sigma Rules</h2>
          <p className="text-sm text-gray-500 mt-1">
            {filteredRules.length === rules.length 
              ? `${rules.length} rules`
              : `${filteredRules.length} of ${rules.length} rules`
            }
          </p>
        </div>
        <div className="flex gap-2">
          <label className="btn-primary cursor-pointer">
            Upload
            <input
              type="file"
              accept=".yaml,.yml"
              onChange={handleFileUpload}
              className="hidden"
            />
          </label>
          <button
            onClick={loadRules}
            className="btn-secondary"
          >
            Refresh
          </button>
        </div>
      </div>

      <RulesFilters 
        rules={rules}
        onFilteredRulesChange={handleFilteredRulesChange}
      />

      <div className="flex-1 overflow-y-auto p-6">
        {filteredRules.length === 0 ? (
          <div className="text-center text-gray-500 py-12">
            {rules.length === 0 ? "No rules found" : "No rules match the current filters"}
          </div>
        ) : (
          <div className="space-y-3">
            {filteredRules.map((rule) => (
              <div
                key={rule.id}
                className="card-bordered"
              >
                <div className="flex items-center justify-between">
                  <div 
                    className="flex-1 cursor-pointer"
                    onClick={() => showRuleDetails(rule.id)}
                  >
                    <div className="text-gray-400 font-mono text-xs mb-1">
                      {rule.id}
                    </div>
                    <div className="flex items-start justify-between mb-2">
                      <div className="font-medium text-gray-900">
                        {rule.title || rule.id}
                      </div>
                      {rule.level && (
                        <span className={`inline-flex px-2 py-1 text-xs font-medium rounded-full ${
                          rule.level.toLowerCase() === 'critical' ? 'bg-red-100 text-red-800' :
                          rule.level.toLowerCase() === 'high' ? 'bg-orange-100 text-orange-800' :
                          rule.level.toLowerCase() === 'medium' ? 'bg-yellow-100 text-yellow-800' :
                          rule.level.toLowerCase() === 'low' ? 'bg-blue-100 text-blue-800' :
                          rule.level.toLowerCase() === 'informational' ? 'bg-green-100 text-green-800' :
                          'bg-gray-100 text-gray-800'
                        }`}>
                          {rule.level}
                        </span>
                      )}
                    </div>
                    
                    <div className="text-sm text-gray-600 mb-2">
                      {rule.description && (
                        <div className="mb-1">{rule.description}</div>
                      )}
                    </div>

                    <div className="flex flex-wrap gap-2 text-xs">
                      {rule.category && (
                        <span className="bg-purple-50 text-purple-700 px-2 py-1 rounded">
                          {rule.category}
                        </span>
                      )}
                      {rule.product && (
                        <span className="bg-blue-50 text-blue-700 px-2 py-1 rounded">
                          {rule.product}
                        </span>
                      )}
                      {rule.service && (
                        <span className="bg-green-50 text-green-700 px-2 py-1 rounded">
                          {rule.service}
                        </span>
                      )}
                      {rule.author && (
                        <span className="bg-gray-50 text-gray-700 px-2 py-1 rounded">
                          by {rule.author}
                        </span>
                      )}
                    </div>

                    <div className="flex items-center gap-2 text-xs mt-2">
                      {rule.logsource?.product && (
                        <span 
                          className="inline-flex px-2 py-1 text-xs font-medium rounded-full"
                          style={{
                            backgroundColor: `${sourceColors[rule.logsource.product]}20`,
                            color: sourceColors[rule.logsource.product] || '#6B7280'
                          }}
                        >
                          {rule.logsource.product}
                        </span>
                      )}
                      {rule.logsource?.service && (
                        <span 
                          className="inline-flex px-2 py-1 text-xs font-medium rounded-full"
                          style={{
                            backgroundColor: `${sourceColors[rule.logsource.service]}20`,
                            color: sourceColors[rule.logsource.service] || '#6B7280'
                          }}
                        >
                          {rule.logsource.service}
                        </span>
                      )}
                    </div>
                  </div>
                  <div className="ml-4">
                    <label className="relative inline-flex items-center cursor-pointer">
                      <input
                        type="checkbox"
                        className="sr-only peer"
                        checked={rule.enabled}
                        onChange={(e) => toggleRule(rule.id, e.target.checked)}
                      />
                      <div className="w-11 h-6 bg-gray-200 peer-focus:outline-none peer-focus:ring-4 peer-focus:ring-blue-300 rounded-full peer peer-checked:after:translate-x-full peer-checked:after:border-white after:content-[''] after:absolute after:top-[2px] after:left-[2px] after:bg-white after:border-gray-300 after:border after:rounded-full after:h-5 after:w-5 after:transition-all peer-checked:bg-green-600"></div>
                    </label>
                  </div>
                </div>
              </div>
            ))}
          </div>
        )}
      </div>

      {/* Rule Details Modal */}
      {selectedRule && (
        <div className="fixed inset-0 bg-black bg-opacity-50 flex items-center justify-center z-50">
          <div className="bg-white rounded-lg max-w-4xl max-h-[80vh] overflow-hidden">
            <div className="px-6 py-4 border-b border-gray-200 flex justify-between items-center">
              <h3 className="text-lg font-semibold">Rule: {selectedRule.id}</h3>
              <button
                onClick={() => setSelectedRule(null)}
                className="text-gray-400 hover:text-gray-600 text-2xl"
              >
                ×
              </button>
            </div>
            <div className="p-6 overflow-y-auto max-h-[60vh]">
              <pre className="bg-gray-50 p-4 rounded border text-sm font-mono whitespace-pre-wrap">
                {selectedRule.content}
              </pre>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
