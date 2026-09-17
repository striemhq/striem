import type { Metadata } from "next";
import { Geist, Geist_Mono } from "next/font/google";
import { FeatureFlagsProvider } from "@/include/features";
import "./globals.css";

const geistSans = Geist({
  variable: "--font-geist-sans",
  subsets: ["latin"],
});

const geistMono = Geist_Mono({
  variable: "--font-geist-mono",
  subsets: ["latin"],
});

export const metadata: Metadata = {
  title: "StrIEM - Sigma Rules Management",
  description: "Manage Sigma rules and sources",
};

export default function RootLayout({
  children,
}: Readonly<{
  children: React.ReactNode;
}>) {
  // Set the theme before first paint, so there is no flash of the wrong colors.
  const themeScript = `(function(){try{var t=localStorage.getItem('striem-theme');if(t!=='light'&&t!=='dark'&&t!=='glitter'){t=window.matchMedia('(prefers-color-scheme: dark)').matches?'dark':'light';}document.documentElement.dataset.theme=t;}catch(e){document.documentElement.dataset.theme='light';}})();`;

  return (
    <html lang="en" suppressHydrationWarning>
      <head>
        <script dangerouslySetInnerHTML={{ __html: themeScript }} />
      </head>
      <body
        className={`${geistSans.variable} ${geistMono.variable} antialiased`}
      >
        <FeatureFlagsProvider>
          {children}
        </FeatureFlagsProvider>
      </body>
    </html>
  );
}
