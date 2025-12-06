import type { NextConfig } from "next";

const nextConfig: NextConfig = {
  /* config options here */
  
  // Enable static export for SPA
  output: 'export',
  
  // Set base path only for production builds
  basePath: process.env.NEXT_PUBLIC_BASE_PATH || '',
  
  // Optimize CSS in production
  experimental: {
    optimizeCss: true,
  },
};

export default nextConfig;
