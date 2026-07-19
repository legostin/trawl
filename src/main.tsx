import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import { ThemeProvider } from "./components/ThemeProvider";
import { useFlows } from "./store";
import "./index.css";

if (import.meta.env.DEV) {
  (window as unknown as { __flows: typeof useFlows }).__flows = useFlows;
}

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <ThemeProvider>
      <App />
    </ThemeProvider>
  </React.StrictMode>,
);
