"use client";

import { invoke } from "@tauri-apps/api/core";
import { useState } from "react";

export function useIndexer() {
  const [isIndexing, setIsIndexing] = useState(false);
  const [indexProgress, setIndexProgress] = useState("");

  const runCodebaseIndex = async (workspaceRoot: string) => {
    setIsIndexing(true);
    try {
      setIndexProgress("Chunking workspace files...");
      const chunkCount = await invoke<number>("build_local_index", { workspaceRoot });
      setIndexProgress(`Chunked ${chunkCount} segments. Generating embeddings...`);
      const embedCount = await invoke<number>("generate_local_embeddings", { workspaceRoot });
      setIndexProgress(`Index complete: ${chunkCount} chunks, ${embedCount} embeddings.`);
    } catch (e: any) {
      setIndexProgress(`Indexing failed: ${e}`);
    } finally {
      setIsIndexing(false);
    }
  };

  return { isIndexing, indexProgress, runCodebaseIndex };
}