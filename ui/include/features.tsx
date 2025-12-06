"use client";

import { createContext, useContext, useEffect, useState, ReactNode } from "react";

interface FeatureFlagsContextType {
  features: string[];
  hasFeature: (feature: string) => boolean;
  isLoading: boolean;
}

const FeatureFlagsContext = createContext<FeatureFlagsContextType>({
  features: [],
  hasFeature: () => false,
  isLoading: true,
});

export function FeatureFlagsProvider({ children }: { children: ReactNode }) {
  const [features, setFeatures] = useState<string[]>([]);
  const [isLoading, setIsLoading] = useState(true);

  useEffect(() => {
    // Fetch feature flags from API on initial page load
    fetch("/api/1/detections")
      .then((response) => {
        const featureHeader = response.headers.get("X-Feature-Flag");
        if (featureHeader) {
          const flags = featureHeader.split(",").map(f => f.trim()).filter(Boolean);
          setFeatures(flags);
        }
      })
      .catch((error) => {
        console.error("Failed to fetch feature flags:", error);
      })
      .finally(() => {
        setIsLoading(false);
      });
  }, []);

  const hasFeature = (feature: string) => features.includes(feature);

  return (
    <FeatureFlagsContext.Provider value={{ features, hasFeature, isLoading }}>
      {children}
    </FeatureFlagsContext.Provider>
  );
}

export function useFeatureFlags() {
  const context = useContext(FeatureFlagsContext);
  if (!context) {
    throw new Error("useFeatureFlags must be used within a FeatureFlagsProvider");
  }
  return context;
}
