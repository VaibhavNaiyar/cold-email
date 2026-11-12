import type { Metadata } from "next";
import { Schibsted_Grotesk, Source_Serif_4 } from "next/font/google";
import "./globals.css";

const grotesk = Schibsted_Grotesk({ subsets: ["latin"], variable: "--font-grotesk" });
const letter = Source_Serif_4({ subsets: ["latin"], variable: "--font-letter" });

export const metadata: Metadata = {
  title: "Startup Scout",
  description: "Newly funded AI startups and the email each one would get.",
};

export default function RootLayout({ children }: LayoutProps<"/">) {
  return (
    <html lang="en" className={`${grotesk.variable} ${letter.variable}`}>
      <body className="antialiased">{children}</body>
    </html>
  );
}
