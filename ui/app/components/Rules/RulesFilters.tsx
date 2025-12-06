"use client";

import { useState, useEffect } from "react";

import { SigmaRule } from "@types";

/*interface SigmaRule {
  id: string;
  hash: string;
  enabled: boolean;
  title?: string;
  level?: string;
  category?: string;
  product?: string;
  service?: string;
  author?: string;
  description?: string;
  tags?: string[];
}*/

interface FilterOptions {
  categories: string[];
  products: string[];
  services: string[];
  levels: string[];
}

interface Filters {
  category: string;
  product: string;
  service: string;
  levels: string[];
  search: string;
}

interface RulesFiltersProps {
  rules: SigmaRule[];
  onFilteredRulesChange: (filteredRules: SigmaRule[]) => void;
}

export default function RulesFilters({ rules, onFilteredRulesChange }: RulesFiltersProps) {
  const [filters, setFilters] = useState<Filters>({
    category: '',
    product: '',
    service: '',
    levels: [],
    search: ''
  });

  const [filterOptions, setFilterOptions] = useState<FilterOptions>({
    categories: [],
    products: [],
    services: [],
    levels: []
  });

  const [isExpanded, setIsExpanded] = useState(false);

  // Extract unique filter options from rules
  useEffect(() => {
    const categories = [...new Set(rules.map(rule => rule.logsource?.category).filter((cat): cat is string => Boolean(cat)))].sort();
    const products = [...new Set(rules.map(rule => rule.logsource?.product).filter((prod): prod is string => Boolean(prod)))].sort();
    const services = [...new Set(rules.map(rule => rule.logsource?.service).filter((serv): serv is string => Boolean(serv)))].sort();
    const levels = [...new Set(rules.map(rule => rule.level).filter((lvl): lvl is string => Boolean(lvl)))].sort();

    setFilterOptions({
      categories,
      products,
      services,
      levels
    });
  }, [rules]);

  // Apply filters whenever filters change or rules change
  useEffect(() => {
    const filteredRules = rules.filter(rule => {
      // Category filter
      if (filters.category && rule.logsource?.category !== filters.category) {
        return false;
      }

      // Product filter
      if (filters.product && rule.logsource?.product !== filters.product) {
        return false;
      }

      // Service filter
      if (filters.service && rule.logsource?.service !== filters.service) {
        return false;
      }

      // Level filter (multiple levels allowed)
      if (filters.levels.length > 0 && rule.level && !filters.levels.includes(rule.level)) {
        return false;
      }

      // Search filter (title, description, tags)
      if (filters.search) {
        const searchLower = filters.search.toLowerCase();
        const matchesTitle = rule.title?.toLowerCase().includes(searchLower);
        const matchesDescription = rule.description?.toLowerCase().includes(searchLower);
        const matchesTags = rule.tags?.some(tag => tag.toLowerCase().includes(searchLower));
        const matchesAuthor = rule.author?.toLowerCase().includes(searchLower);
        
        if (!matchesTitle && !matchesDescription && !matchesTags && !matchesAuthor) {
          return false;
        }
      }

      return true;
    });

    onFilteredRulesChange(filteredRules);
  }, [filters, rules, onFilteredRulesChange]);

  const updateFilter = (key: keyof Filters, value: string) => {
    setFilters(prev => ({
      ...prev,
      [key]: value
    }));
  };

  const toggleLevel = (level: string) => {
    setFilters(prev => ({
      ...prev,
      levels: prev.levels.includes(level)
        ? prev.levels.filter(l => l !== level)
        : [...prev.levels, level]
    }));
  };

  const clearFilters = () => {
    setFilters({
      category: '',
      product: '',
      service: '',
      levels: [],
      search: ''
    });
  };

  const hasActiveFilters = filters.category !== '' || filters.product !== '' || filters.service !== '' || filters.levels.length > 0 || filters.search !== '';
  const activeFilterCount = [
    filters.category !== '',
    filters.product !== '', 
    filters.service !== '',
    filters.levels.length > 0,
    filters.search !== ''
  ].filter(Boolean).length;

  return (
    <div className="bg-white border-b border-gray-200">
      <div className="px-6 py-4">
        <div className="flex items-center justify-between">
          <div className="flex items-center space-x-4">
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
                  placeholder="Search rules..."
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
          <div className="mt-4 grid grid-cols-1 md:grid-cols-2 lg:grid-cols-4 gap-4">
            <div>
              <label className="input-label">Category</label>
              <select
                value={filters.category}
                onChange={(e) => updateFilter('category', e.target.value)}
                className="input-select"
              >
                <option value="">All Categories</option>
                {filterOptions.categories.map(category => (
                  <option key={category} value={category}>{category}</option>
                ))}
              </select>
            </div>

            <div>
              <label className="input-label">Product</label>
              <select
                value={filters.product}
                onChange={(e) => updateFilter('product', e.target.value)}
                className="input-select"
              >
                <option value="">All Products</option>
                {filterOptions.products.map(product => (
                  <option key={product} value={product}>{product}</option>
                ))}
              </select>
            </div>

            <div>
              <label className="input-label">Service</label>
              <select
                value={filters.service}
                onChange={(e) => updateFilter('service', e.target.value)}
                className="input-select"
              >
                <option value="">All Services</option>
                {filterOptions.services.map(service => (
                  <option key={service} value={service}>{service}</option>
                ))}
              </select>
            </div>

            <div>
              <div className="flex items-center gap-2 mb-1">
                <label className="block text-sm font-medium text-gray-700">Level</label>
                {filters.levels.length > 0 && (
                  <span className="bg-blue-100 text-blue-800 text-xs font-medium px-2 py-0.5 rounded">
                    {filters.levels.length}
                  </span>
                )}
              </div>
              <div className="flex items-center justify-between mb-2">
                <p className="text-xs text-gray-500">Click to select multiple levels</p>
                {filters.levels.length > 0 && (
                  <button
                    onClick={() => setFilters(prev => ({ ...prev, levels: [] }))}
                    className="text-xs text-blue-600 hover:text-blue-800"
                  >
                    Clear levels ({filters.levels.length})
                  </button>
                )}
              </div>
              <div className="flex flex-wrap gap-2">
                {filterOptions.levels.map(level => {
                  const isSelected = filters.levels.includes(level);
                  let levelColor = 'bg-gray-100 text-gray-800 border-gray-200';
                  
                  if (isSelected) {
                    switch (level.toLowerCase()) {
                      case 'critical':
                        levelColor = 'bg-red-500 text-white border-red-500';
                        break;
                      case 'high':
                        levelColor = 'bg-orange-500 text-white border-orange-500';
                        break;
                      case 'medium':
                        levelColor = 'bg-yellow-500 text-white border-yellow-500';
                        break;
                      case 'low':
                        levelColor = 'bg-blue-500 text-white border-blue-500';
                        break;
                      case 'informational':
                        levelColor = 'bg-green-500 text-white border-green-500';
                        break;
                      default:
                        levelColor = 'bg-gray-500 text-white border-gray-500';
                    }
                  } else {
                    switch (level.toLowerCase()) {
                      case 'critical':
                        levelColor = 'bg-red-50 text-red-700 border-red-200 hover:bg-red-100';
                        break;
                      case 'high':
                        levelColor = 'bg-orange-50 text-orange-700 border-orange-200 hover:bg-orange-100';
                        break;
                      case 'medium':
                        levelColor = 'bg-yellow-50 text-yellow-700 border-yellow-200 hover:bg-yellow-100';
                        break;
                      case 'low':
                        levelColor = 'bg-blue-50 text-blue-700 border-blue-200 hover:bg-blue-100';
                        break;
                      case 'informational':
                        levelColor = 'bg-green-50 text-green-700 border-green-200 hover:bg-green-100';
                        break;
                      default:
                        levelColor = 'bg-gray-50 text-gray-700 border-gray-200 hover:bg-gray-100';
                    }
                  }

                  return (
                    <button
                      key={level}
                      onClick={() => toggleLevel(level)}
                      className={`px-3 py-1 text-xs font-medium rounded-full border-2 transition-colors ${levelColor} ${
                        isSelected ? 'ring-2 ring-offset-1 ring-gray-400' : ''
                      }`}
                      title={isSelected ? `Remove ${level} filter` : `Add ${level} filter`}
                    >
                      {isSelected && '✓ '}{level}
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
