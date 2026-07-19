import { useEffect, useState } from "react";
import { TrafficList } from "./components/TrafficList";
import { FlowDetail } from "./components/FlowDetail";
import { SetupPanel } from "./components/SetupPanel";
import { FilterBar } from "./components/FilterBar";
import { useFlows } from "./store";
import "./App.css";

type View = "traffic" | "setup";

function App() {
  const init = useFlows((s) => s.init);
  const startProxy = useFlows((s) => s.startProxy);
  const stopProxy = useFlows((s) => s.stopProxy);
  const [running, setRunning] = useState(false);
  const [addr, setAddr] = useState<string>("");
  const [view, setView] = useState<View>("traffic");

  useEffect(() => {
    let cleanup: (() => void) | undefined;
    init().then((c) => (cleanup = c));
    return () => cleanup?.();
  }, [init]);

  const toggle = async () => {
    if (running) {
      await stopProxy();
      setRunning(false);
      setAddr("");
    } else {
      const a = await startProxy(8888);
      setRunning(true);
      setAddr(a);
    }
  };

  return (
    <div
      style={{
        display: "flex",
        flexDirection: "column",
        height: "100vh",
        color: "#ddd",
        background: "#1e1e1e",
      }}
    >
      <div
        style={{
          display: "flex",
          alignItems: "center",
          gap: 12,
          padding: 8,
          borderBottom: "1px solid #333",
        }}
      >
        <button onClick={toggle}>{running ? "Stop" : "Start"} proxy</button>
        {addr && <span>Proxy: {addr}</span>}
        <div style={{ flex: 1 }} />
        <button
          onClick={() => setView("traffic")}
          style={{ fontWeight: view === "traffic" ? "bold" : "normal" }}
        >
          Traffic
        </button>
        <button
          onClick={() => setView("setup")}
          style={{ fontWeight: view === "setup" ? "bold" : "normal" }}
        >
          Setup
        </button>
      </div>

      {view === "setup" ? (
        <SetupPanel />
      ) : (
        <div style={{ display: "flex", flex: 1, minHeight: 0 }}>
          <div
            style={{
              width: "45%",
              borderRight: "1px solid #333",
              display: "flex",
              flexDirection: "column",
              minHeight: 0,
            }}
          >
            <FilterBar />
            <div style={{ flex: 1, minHeight: 0 }}>
              <TrafficList />
            </div>
          </div>
          <div style={{ flex: 1, minWidth: 0 }}>
            <FlowDetail />
          </div>
        </div>
      )}
    </div>
  );
}

export default App;
