"use client";

import { useEffect, useState } from "react";

export default function BootstrapManager() {
  const [status, setStatus] = useState("Waking AI Engine...");
  const [visible, setVisible] = useState(true);

  useEffect(() => {
    const initEngine = async () => {
      try {
        setStatus("Verifying local model availability...");
        const res = await fetch("http://localhost:11434/api/tags");
        if (!res.ok) throw new Error("Ollama not running");
        setStatus("AI Engine Ready.");
        setTimeout(() => setVisible(false), 2000);
      } catch {
        setVisible(false);
      }
    };
    initEngine();
  }, []);

  if (!visible) return null;

  return (
    <div className="fixed bottom-4 right-4 bg-gray-900 border border-gray-700 p-4 rounded-lg shadow-lg z-50 text-white text-sm">
      <div className="flex items-center space-x-3">
        <div className="animate-spin h-4 w-4 border-2 border-purple-500 border-t-transparent rounded-full" />
        <span>{status}</span>
      </div>
    </div>
  );
}