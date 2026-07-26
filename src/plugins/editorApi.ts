import * as monaco from "monaco-editor";
import type { CompletionItem, TrawlEditor } from "./api";

const KIND: Record<NonNullable<CompletionItem["kind"]>, monaco.languages.CompletionItemKind> = {
  function: monaco.languages.CompletionItemKind.Function,
  variable: monaco.languages.CompletionItemKind.Variable,
  file: monaco.languages.CompletionItemKind.File,
  snippet: monaco.languages.CompletionItemKind.Snippet,
  keyword: monaco.languages.CompletionItemKind.Keyword,
};

/**
 * Lets a plugin offer completions in the host's editor. The plugin sees only the
 * line prefix and the document text — no Monaco types leak across the boundary,
 * so the editor can be swapped without breaking plugins.
 */
export function editorApi(): TrawlEditor {
  return {
    registerCompletions: (spec) => {
      const provider = monaco.languages.registerCompletionItemProvider(spec.language ?? "javascript", {
        triggerCharacters: spec.triggerCharacters,
        provideCompletionItems(model, position) {
          const linePrefix = model.getValueInRange({
            startLineNumber: position.lineNumber,
            startColumn: 1,
            endLineNumber: position.lineNumber,
            endColumn: position.column,
          });

          const word = model.getWordUntilPosition(position);
          const range = {
            startLineNumber: position.lineNumber,
            endLineNumber: position.lineNumber,
            startColumn: word.startColumn,
            endColumn: word.endColumn,
          };

          let items: CompletionItem[] = [];
          try {
            items = spec.provide({ linePrefix, text: model.getValue() });
          } catch {
            // A plugin's bad provider must not break typing in the editor.
            return { suggestions: [] };
          }

          return {
            suggestions: items.map((item) => ({
              label: item.label,
              kind: KIND[item.kind ?? "function"],
              insertText: item.insert ?? item.label,
              insertTextRules: (item.insert ?? "").includes("$0")
                ? monaco.languages.CompletionItemInsertTextRule.InsertAsSnippet
                : undefined,
              detail: item.detail,
              documentation: item.documentation,
              range,
            })),
          };
        },
      });
      return () => provider.dispose();
    },
  };
}
