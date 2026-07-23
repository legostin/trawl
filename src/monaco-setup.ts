import * as monaco from "monaco-editor";
import editorWorker from "monaco-editor/esm/vs/editor/editor.worker?worker";
import tsWorker from "monaco-editor/esm/vs/language/typescript/ts.worker?worker";
import { loader } from "@monaco-editor/react";
import { API_DTS } from "./scripting/apiTypes";
import { STD_DTS } from "./scripting/stdlib";
import { registerPathHints } from "./scripting/pathHints";

// Offline workers (Tauri without a CDN).
(self as unknown as { MonacoEnvironment: unknown }).MonacoEnvironment = {
  getWorker(_moduleId: string, label: string) {
    if (label === "typescript" || label === "javascript") return new tsWorker();
    return new editorWorker();
  },
};

// Use the locally bundled monaco, not a CDN.
loader.config({ monaco });

// The `languages.typescript` type is marked deprecated in this build, though it's available at runtime.
interface TsDefaults {
  addExtraLib(content: string, path?: string): { dispose: () => void };
  setDiagnosticsOptions(opts: { noSemanticValidation?: boolean; noSyntaxValidation?: boolean }): void;
}
const jsDefaults = (
  monaco.languages as unknown as { typescript: { javascriptDefaults: TsDefaults } }
).typescript.javascriptDefaults;

// Autocomplete for our scripting API + standard library.
jsDefaults.addExtraLib(API_DTS, "ts:trawl-api.d.ts");
jsDefaults.addExtraLib(STD_DTS, "ts:trawl-stdlib.d.ts");
jsDefaults.setDiagnosticsOptions({
  noSemanticValidation: true, // don't complain about "redeclaring" globals from the d.ts
  noSyntaxValidation: false,
});

let libDisposable: { dispose: () => void } | null = null;

/** Updates autocomplete with functions from the library prelude. */
export function setLibraryTypes(source: string) {
  libDisposable?.dispose();
  libDisposable = jsDefaults.addExtraLib(source, "ts:trawl-library.js");
}

let dataDisposable: { dispose: () => void } | null = null;

/** Types `response.data` (from sendJsonRequest) by the structure of past
 *  responses matching the current rule. `typeBody` comes from fieldsToType(). */
export function setResponseDataType(typeBody: string) {
  dataDisposable?.dispose();
  dataDisposable = jsDefaults.addExtraLib(
    `type TrawlResponseData = ${typeBody};`,
    "ts:trawl-response-data.d.ts",
  );
}

// Default until a rule is selected.
setResponseDataType("{ [key: string]: any }");

registerPathHints(monaco);

let payloadDisposable: { dispose: () => void } | null = null;

/** Types the global `payload` for event-subscription editors (plugins). */
export function setEventPayloadType(typeBody: string) {
  payloadDisposable?.dispose();
  payloadDisposable = jsDefaults.addExtraLib(
    `declare const payload: ${typeBody};`,
    "ts:trawl-event-payload.d.ts",
  );
}

export { monaco };
