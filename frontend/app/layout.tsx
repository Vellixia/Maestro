import type { Metadata } from "next";
import { Geist, Geist_Mono } from "next/font/google";
import Link from "next/link";
import "./globals.css";

const geistSans = Geist({ variable: "--font-geist-sans", subsets: ["latin"] });
const geistMono = Geist_Mono({ variable: "--font-geist-mono", subsets: ["latin"] });

export const metadata: Metadata = {
  title: "Maestro — AI Orchestration",
  description: "Capability-aware LLM orchestration platform",
};

export default function RootLayout({ children }: { children: React.ReactNode }) {
  return (
    <html lang="en" className={`${geistSans.variable} ${geistMono.variable} h-full antialiased`}>
      <body className="min-h-full flex flex-col">
        <header className="border-b border-[var(--card-border)] px-6 py-3 flex items-center gap-6">
          <Link href="/" className="font-bold text-lg tracking-tight text-[var(--accent-light)]">
            🎼 Maestro
          </Link>
          <nav className="flex gap-4 text-sm text-[var(--muted)]">
            <Link href="/" className="hover:text-[var(--foreground)] transition-colors">Dashboard</Link>
            <Link href="/runs" className="hover:text-[var(--foreground)] transition-colors">Runs</Link>
            <Link href="/orchestrate" className="hover:text-[var(--foreground)] transition-colors">Orchestrate</Link>
          </nav>
        </header>
        <main className="flex-1 p-6">{children}</main>
      </body>
    </html>
  );
}
