"use client";

import { useState, useRef } from "react";

interface QueryResult {
  data: Array<Record<string, any>>;
  rowCount: number;
  executionTime: number;
}

interface QueryHistory {
  id: string;
  sql: string;
  timestamp: Date;
  status: "success" | "error" | "running";
  executionTime?: number;
  error?: string;
}

export default function ExploreTab() {
  const [sql, setSql] = useState<string>("SELECT * FROM findings/detection_finding/**/*.parquet LIMIT 100;");
  const [queryResult, setQueryResult] = useState<QueryResult | null>(null);
  const [isExecuting, setIsExecuting] = useState(false);
  const [error, setError] = useState<string>("");
  const [queryHistory, setQueryHistory] = useState<QueryHistory[]>([]);
  const [activeTab, setActiveTab] = useState<"results" | "history">("results");
  const [limit, setLimit] = useState<number>(100);
  const textAreaRef = useRef<HTMLTextAreaElement>(null);

  const executeQuery = async () => {
    if (!sql.trim()) {
      setError("Please enter a SQL query");
      return;
    }

    setIsExecuting(true);
    setError("");
    
    const queryId = Date.now().toString();
    const newHistoryEntry: QueryHistory = {
      id: queryId,
      sql: sql,
      timestamp: new Date(),
      status: "running",
    };
    
    setQueryHistory(prev => [newHistoryEntry, ...prev]);

    try {
      const startTime = Date.now();
      const response = await fetch(`${process.env.NEXT_PUBLIC_API_URL}/query`, {
        method: "POST",
        headers: {
          "Content-Type": "application/json",
        },
        body: JSON.stringify({ sql, limit }),
      });

      if (!response.ok) {
        const errorData = await response.json();
        throw new Error(errorData.error || "Query execution failed");
      }

      // API returns an array of JSON objects
      const data = await response.json();
      console.log("Query Data:", data);
      const executionTime = Date.now() - startTime;

      // Transform array response to our result format
      const result: QueryResult = {
        data: Array.isArray(data) ? data : [],
        rowCount: Array.isArray(data) ? data.length : 0,
        executionTime,
      };

      setQueryResult(result);
      setQueryHistory(prev => 
        prev.map(item => 
          item.id === queryId 
            ? { ...item, status: "success", executionTime: result.executionTime }
            : item
        )
      );
      setActiveTab("results");
    } catch (err: any) {
      const errorMessage = err.message || "An error occurred while executing the query";
      setError(errorMessage);
      setQueryHistory(prev => 
        prev.map(item => 
          item.id === queryId 
            ? { ...item, status: "error", error: errorMessage }
            : item
        )
      );
    } finally {
      setIsExecuting(false);
    }
  };

  const handleKeyDown = (e: React.KeyboardEvent) => {
    // Cmd+Enter or Ctrl+Enter to execute
    if ((e.metaKey || e.ctrlKey) && e.key === "Enter") {
      e.preventDefault();
      executeQuery();
    }
  };

  const loadFromHistory = (historySql: string) => {
    setSql(historySql);
    if (textAreaRef.current) {
      textAreaRef.current.focus();
    }
  };

  return (
    <div className="flex-1 flex flex-col h-full overflow-hidden">
      {/* Header */}
      <div className="p-4 border-b border-gray-200 bg-white">
        <div className="flex items-center justify-between">
          <div>
            <h2 className="text-2xl font-bold text-gray-900">SQL Lab</h2>
            <p className="text-sm text-gray-600 mt-1">Query and explore your data with SQL</p>
          </div>
        </div>
      </div>

      {/* Main Content Area */}
      <div className="flex-1 flex flex-col min-h-0">
        {/* SQL Editor Section */}
        <div className="flex flex-col border-b border-gray-200 bg-white">
          {/* Toolbar */}
          <div className="px-4 py-2 border-b border-gray-200 flex items-center gap-2 bg-gray-50">
            <button
              onClick={executeQuery}
              disabled={isExecuting}
              className={`px-4 py-1.5 rounded text-sm font-medium transition-colors ${
                isExecuting
                  ? "bg-gray-300 text-gray-500 cursor-not-allowed"
                  : "bg-blue-600 text-white hover:bg-blue-700"
              }`}
            >
              {isExecuting ? (
                <span className="flex items-center gap-2">
                  <span className="inline-block w-4 h-4 border-2 border-white border-t-transparent rounded-full animate-spin"></span>
                  Running...
                </span>
              ) : (
                <span className="flex items-center gap-2">
                  ▶ Run Query
                  <span className="text-xs opacity-75">(⌘+Enter)</span>
                </span>
              )}
            </button>
            
            <div className="flex items-center gap-2 ml-4">
              <label htmlFor="limit" className="text-sm text-gray-600">
                Limit:
              </label>
              <input
                type="number"
                id="limit"
                value={limit}
                onChange={(e) => setLimit(parseInt(e.target.value) || 100)}
                className="w-20 px-2 py-1 border border-gray-300 rounded text-sm"
                min="1"
                max="10000"
              />
            </div>
          </div>

          {/* SQL Editor */}
          <div className="p-4">
            <textarea
              ref={textAreaRef}
              value={sql}
              onChange={(e) => setSql(e.target.value)}
              onKeyDown={handleKeyDown}
              className="w-full h-48 p-3 font-mono text-sm border border-gray-300 rounded-md focus:outline-none focus:ring-2 focus:ring-blue-500 focus:border-blue-500 resize-y"
              placeholder="Enter your SQL query here..."
              spellCheck={false}
            />
          </div>
        </div>

        {/* Results Section */}
        <div className="flex-1 flex flex-col min-h-0 bg-gray-50">
          {/* Tabs */}
          <div className="border-b border-gray-200 bg-white">
            <div className="flex">
              <button
                onClick={() => setActiveTab("results")}
                className={`px-6 py-3 text-sm font-medium border-b-2 transition-colors ${
                  activeTab === "results"
                    ? "border-blue-600 text-blue-600"
                    : "border-transparent text-gray-600 hover:text-gray-900"
                }`}
              >
                Results
                {queryResult && (
                  <span className="ml-2 text-xs bg-gray-200 px-2 py-0.5 rounded-full">
                    {queryResult.rowCount} rows
                  </span>
                )}
              </button>
              <button
                onClick={() => setActiveTab("history")}
                className={`px-6 py-3 text-sm font-medium border-b-2 transition-colors ${
                  activeTab === "history"
                    ? "border-blue-600 text-blue-600"
                    : "border-transparent text-gray-600 hover:text-gray-900"
                }`}
              >
                Query History
                {queryHistory.length > 0 && (
                  <span className="ml-2 text-xs bg-gray-200 px-2 py-0.5 rounded-full">
                    {queryHistory.length}
                  </span>
                )}
              </button>
            </div>
          </div>

          {/* Tab Content */}
          <div className="flex-1 overflow-auto p-4">
            {activeTab === "results" && (
              <div>
                {error && (
                  <div className="mb-4 bg-red-50 border border-red-200 rounded-lg p-4">
                    <div className="flex items-start">
                      <div className="flex-shrink-0">
                        <span className="text-red-400 text-xl">⚠️</span>
                      </div>
                      <div className="ml-3">
                        <h3 className="text-sm font-medium text-red-800 mb-1">Query Error</h3>
                        <p className="text-sm text-red-700">{error}</p>
                      </div>
                    </div>
                  </div>
                )}

                {queryResult ? (
                  <div className="bg-white rounded-lg shadow overflow-hidden">
                    <div className="px-4 py-3 border-b border-gray-200 bg-gray-50">
                      <div className="flex items-center justify-between text-sm">
                        <span className="text-gray-600">
                          Query completed in <span className="font-medium">{queryResult.executionTime}ms</span>
                        </span>
                        <span className="text-gray-600">
                          {queryResult.rowCount} rows returned
                        </span>
                      </div>
                    </div>
                    <div className="overflow-x-auto">
                      <table className="min-w-full divide-y divide-gray-200">
                        <thead className="bg-gray-50">
                          <tr>
                            {queryResult.data.length > 0 && Object.keys(queryResult.data[0]).map((column, idx) => (
                              <th
                                key={idx}
                                className="px-4 py-3 text-left text-xs font-medium text-gray-500 uppercase tracking-wider whitespace-nowrap"
                              >
                                {column}
                              </th>
                            ))}
                          </tr>
                        </thead>
                        <tbody className="bg-white divide-y divide-gray-200">
                          {queryResult.data.map((row, rowIdx) => (
                            <tr key={rowIdx} className="hover:bg-gray-50">
                              {Object.values(row).map((cell, cellIdx) => (
                                <td
                                  key={cellIdx}
                                  className="px-4 py-3 text-sm text-gray-900 whitespace-nowrap"
                                >
                                  {cell === null ? (
                                    <span className="text-gray-400 italic">null</span>
                                  ) : typeof cell === "object" ? (
                                    <pre className="text-xs">{JSON.stringify(cell, null, 2)}</pre>
                                  ) : (
                                    String(cell)
                                  )}
                                </td>
                              ))}
                            </tr>
                          ))}
                        </tbody>
                      </table>
                    </div>
                  </div>
                ) : !error && (
                  <div className="text-center py-12 text-gray-500">
                    <svg
                      className="mx-auto h-12 w-12 text-gray-400 mb-4"
                      fill="none"
                      viewBox="0 0 24 24"
                      stroke="currentColor"
                    >
                      <path
                        strokeLinecap="round"
                        strokeLinejoin="round"
                        strokeWidth={2}
                        d="M9 12h6m-6 4h6m2 5H7a2 2 0 01-2-2V5a2 2 0 012-2h5.586a1 1 0 01.707.293l5.414 5.414a1 1 0 01.293.707V19a2 2 0 01-2 2z"
                      />
                    </svg>
                    <p>Execute a query to see results</p>
                  </div>
                )}
              </div>
            )}

            {activeTab === "history" && (
              <div className="space-y-2">
                {queryHistory.length === 0 ? (
                  <div className="text-center py-12 text-gray-500">
                    <svg
                      className="mx-auto h-12 w-12 text-gray-400 mb-4"
                      fill="none"
                      viewBox="0 0 24 24"
                      stroke="currentColor"
                    >
                      <path
                        strokeLinecap="round"
                        strokeLinejoin="round"
                        strokeWidth={2}
                        d="M12 8v4l3 3m6-3a9 9 0 11-18 0 9 9 0 0118 0z"
                      />
                    </svg>
                    <p>No query history yet</p>
                  </div>
                ) : (
                  queryHistory.map((item) => (
                    <div
                      key={item.id}
                      className="bg-white rounded-lg border border-gray-200 p-4 hover:shadow-md transition-shadow"
                    >
                      <div className="flex items-start justify-between mb-2">
                        <div className="flex items-center gap-2">
                          <span
                            className={`inline-block w-2 h-2 rounded-full ${
                              item.status === "success"
                                ? "bg-green-500"
                                : item.status === "error"
                                ? "bg-red-500"
                                : "bg-yellow-500 animate-pulse"
                            }`}
                          ></span>
                          <span className="text-xs text-gray-500">
                            {item.timestamp.toLocaleString()}
                          </span>
                          {item.executionTime && (
                            <span className="text-xs text-gray-500">
                              • {item.executionTime}ms
                            </span>
                          )}
                        </div>
                        <button
                          onClick={() => loadFromHistory(item.sql)}
                          className="text-xs text-blue-600 hover:text-blue-800 font-medium"
                        >
                          Load Query
                        </button>
                      </div>
                      <pre className="text-sm font-mono bg-gray-50 p-2 rounded overflow-x-auto">
                        {item.sql}
                      </pre>
                      {item.error && (
                        <div className="mt-2 text-sm text-red-600">
                          Error: {item.error}
                        </div>
                      )}
                    </div>
                  ))
                )}
              </div>
            )}
          </div>
        </div>
      </div>
    </div>
  );
}
