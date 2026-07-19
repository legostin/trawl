import * as monaco from "monaco-editor";
import editorWorker from "monaco-editor/esm/vs/editor/editor.worker?worker";
import tsWorker from "monaco-editor/esm/vs/language/typescript/ts.worker?worker";
import { loader } from "@monaco-editor/react";

// Оффлайн-воркеры (Tauri без CDN).
(self as unknown as { MonacoEnvironment: unknown }).MonacoEnvironment = {
  getWorker(_moduleId: string, label: string) {
    if (label === "typescript" || label === "javascript") return new tsWorker();
    return new editorWorker();
  },
};

// Использовать локально забандленный monaco, а не CDN.
loader.config({ monaco });

export { monaco };
