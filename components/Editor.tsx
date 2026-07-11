"use client";

import { useCallback, useEffect, useRef } from "react";
import MonacoEditor, { OnMount } from "@monaco-editor/react";
import { useTheme } from "@/hooks/useTheme";
import { useGhostText } from "@/hooks/useGhostText";
import { useInlinePrompt } from "@/hooks/useInlinePrompt";
import { useInlineDiff } from "@/hooks/useInlineDiff";
import InlinePromptBar from "@/components/editor/InlinePromptBar";
import { dispatchInlineRefactor } from "@/lib/ai";

export interface EditorProps {
  path: string;
  language: string;
  value: string;
  onChange: (value: string) => void;
  onSave: () => void;
}

export default function Editor({ path, language, value, onChange, onSave }: EditorProps) {
  const onSaveRef = useRef(onSave);
  onSaveRef.current = onSave;
  const onChangeRef = useRef(onChange);
  onChangeRef.current = onChange;
  const { theme } = useTheme();
  const { ghost, acceptGhost, dismissGhost } = useGhostText();
  const { state: prompt, open, close } = useInlinePrompt();
  const { diffState, clearDiff } = useInlineDiff();
  const editorRef = useRef<Parameters<OnMount>[0] | null>(null);
  const monacoRef = useRef<any>(null);
  const decorationsRef = useRef<string[]>([]);

  // Apply inline diff decorations as they stream in
  useEffect(() => {
    const editor = editorRef.current;
    const monaco = monacoRef.current;
    if (!editor || !monaco || !diffState.active || diffState.lines.length === 0) return;

    const decorations: { range: any; options: any }[] = [];
    for (const dl of diffState.lines) {
      // Monaco is 1-indexed for line numbers
      const lineNum = dl.line_index + 1;
      if (dl.op === "Insert") {
        decorations.push({
          range: new monaco.Range(lineNum, 1, lineNum, 1),
          options: {
            isWholeLine: true,
            linesDecorationsClassName: "diff-insert-line",
            className: "diff-insert-line",
          },
        });
      } else if (dl.op === "Delete") {
        decorations.push({
          range: new monaco.Range(lineNum, 1, lineNum, 1),
          options: {
            isWholeLine: true,
            linesDecorationsClassName: "diff-delete-line",
            className: "diff-delete-line",
          },
        });
      }
    }
    decorationsRef.current = editor.deltaDecorations(decorationsRef.current, decorations);
  }, [diffState]);

  // Accept/Reject keybindings for diff state
  const handleDiffKeyDown = useCallback((e: KeyboardEvent) => {
    if (!diffState.active || !editorRef.current || !monacoRef.current) return;

    // Ctrl+Enter or Cmd+Enter: Accept diff
    if (e.key === "Enter" && (e.ctrlKey || e.metaKey)) {
      e.preventDefault();
      const editor = editorRef.current;
      const monaco = monacoRef.current;

      // Build edits: collect Insert lines and remove Delete lines
      const edits: any[] = [];
      let insertText = "";
      for (const dl of diffState.lines) {
        if (dl.op === "Insert") {
          insertText += dl.line + "\n";
        }
        // Delete lines are handled by not including them in the final result
      }
      if (insertText) {
        const model = editor.getModel();
        if (model) {
          const lastLine = model.getLineCount();
          edits.push({
            range: new monaco.Range(lastLine + 1, 1, lastLine + 1, 1),
            text: insertText,
            forceMoveMarkers: true,
          });
        }
      }
      if (edits.length > 0) {
        editor.executeEdits("diff-accept", edits);
        onChangeRef.current(editor.getValue());
      }
      editor.focus();
      return;
    }

    // Escape: Reject diff (clear all decoration state)
    if (e.key === "Escape") {
      const editor = editorRef.current;
      editor.deltaDecorations(decorationsRef.current, []);
      decorationsRef.current = [];
      clearDiff();
    }
  }, [diffState, clearDiff]);

  useEffect(() => {
    window.addEventListener("keydown", handleDiffKeyDown);
    return () => window.removeEventListener("keydown", handleDiffKeyDown);
  }, [handleDiffKeyDown]);

  // Accept ghost text on Tab, dismiss on Esc or printable char
  const handleKeyDown = useCallback((e: KeyboardEvent) => {
    if (!ghost.active || !editorRef.current) return;

    if (e.key === "Tab") {
      e.preventDefault();
      const insertion = acceptGhost();
      if (!insertion) return;
      const editor = editorRef.current;
      const position = editor.getPosition();
      if (!position) return;
      editor.executeEdits("ghost-text", [
        { range: new (window as any).monaco.Range(position.lineNumber, position.column, position.lineNumber, position.column), text: insertion, forceMoveMarkers: true },
      ]);
      editor.setPosition({ lineNumber: position.lineNumber, column: position.column + insertion.length });
      editor.focus();
    } else if (e.key === "Escape" || e.key.length === 1) {
      dismissGhost();
    }
  }, [ghost.active, acceptGhost, dismissGhost]);

  useEffect(() => {
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [handleKeyDown]);

  const handleMount: OnMount = (editor, monacoInstance) => {
    editorRef.current = editor;
    monacoRef.current = monacoInstance;
    (window as any).monaco = monacoInstance;
    editor.focus();

    const editorDom = editor.getDomNode();
    if (editorDom?.parentElement) {
      // Use editor container ref from closure - stored as instance property
    }

    editor.addCommand(monacoInstance.KeyMod.CtrlCmd | monacoInstance.KeyCode.KeyS, () => {
      onSaveRef.current();
    });

    // Cmd+K / Ctrl+K: Inline Prompt
    editor.addAction({
      id: "neuralforge-inline-prompt",
      label: "Open Inline AI Prompt",
      keybindings: [monacoInstance.KeyMod.CtrlCmd | monacoInstance.KeyCode.KeyK],
      run: (ed) => {
        const selection = ed.getSelection();
        if (!selection) return;

        const selectedText = ed.getModel()?.getValueInRange(selection) ?? "";
        const cursorLine = selection.positionLineNumber;

        const visiblePos = ed.getScrolledVisiblePosition({ lineNumber: cursorLine, column: 1 });
        if (!visiblePos) return;

        const container = ed.getDomNode()?.parentElement;
        if (!container) return;
        const containerRect = container.getBoundingClientRect();
        const x = containerRect.left + (visiblePos.left ?? 0);
        const y = containerRect.top + (visiblePos.top ?? 0) + 22;

        open(x, y, selectedText, cursorLine).then((result) => {
          if (result && editor.getModel()) {
            const pos = editor.getPosition();
            if (!pos) return;
            editor.executeEdits("ai-prompt", [
              { range: new monacoInstance.Range(pos.lineNumber, pos.column, pos.lineNumber, pos.column), text: result, forceMoveMarkers: true },
            ]);
            onChangeRef.current(ed.getValue());
          }
          editor.focus();
        });
      },
    });
  };

  return (
    <>
      <MonacoEditor
        path={path}
        language={language}
        value={value}
        theme={theme === "dark" ? "vs-dark" : "light"}
        onMount={handleMount}
        onChange={(v) => onChange(v ?? "")}
        loading={<div className="h-full w-full bg-white dark:bg-[#1e1e1e]" />}
        options={{
          minimap: { enabled: true },
          fontSize: 13,
          automaticLayout: true,
          scrollBeyondLastLine: false,
        }}
      />
      {prompt.isOpen && (
        <InlinePromptBar
          x={prompt.x}
          y={prompt.y}
          initialValue={prompt.selectedText ? `refactor: ${prompt.selectedText}` : ""}
          onSubmit={async (v) => {
            const payload = { file_path: path, selected_code: prompt.selectedText, user_instruction: v };
            try {
              const resp = await dispatchInlineRefactor(payload);
              if (resp.generated_code) { close(resp.generated_code); }
              else { close(null); }
            } catch { close(null); }
            return null;
          }}
          onClose={() => close(null)}
        />
      )}
    </>
  );
}