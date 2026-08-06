import type { Metadata } from "next";
import Link from "next/link";
import "./globals.css";

export const metadata: Metadata = {
  title: "LingBi",
  description: "本地优先的 AI 小说写作工具",
};

const nav = [
  ["首页", "/"],
  ["下载", "/download"],
  ["定价", "/pricing"],
  ["登录", "/login"],
  ["账户", "/account"],
  ["发布", "/releases"],
  ["隐私", "/privacy"],
  ["条款", "/terms"],
] as const;

export default function RootLayout({
  children,
}: Readonly<{ children: React.ReactNode }>) {
  return (
    <html lang="zh-CN">
      <body>
        <header className="site-header">
          <Link href="/" className="brand">
            LingBi
          </Link>
          <nav>
            {nav.map(([label, href]) => (
              <Link key={href} href={href}>
                {label}
              </Link>
            ))}
          </nav>
        </header>
        <main>{children}</main>
        <footer className="site-footer">LingBi Next</footer>
      </body>
    </html>
  );
}
