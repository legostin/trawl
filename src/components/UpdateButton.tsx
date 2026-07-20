import { Download, Loader2, RefreshCw } from "lucide-react";
import { useUpdater } from "../updater";
import { Button } from "./ui/button";

/** Top-bar update control: an "Update to vX" pill when available, else a manual check. */
export function UpdateButton() {
  const status = useUpdater((s) => s.status);
  const version = useUpdater((s) => s.version);
  const progress = useUpdater((s) => s.progress);
  const check = useUpdater((s) => s.check);
  const install = useUpdater((s) => s.install);

  if (status === "available") {
    return (
      <Button
        variant="default"
        size="sm"
        title={`Download and install version ${version}`}
        onClick={() => void install()}
      >
        <Download />
        Update to v{version}
      </Button>
    );
  }

  if (status === "downloading") {
    return (
      <Button variant="default" size="sm" disabled>
        <Loader2 className="animate-spin" />
        Downloading… {progress}%
      </Button>
    );
  }

  if (status === "ready") {
    return (
      <Button variant="default" size="sm" disabled>
        <Loader2 className="animate-spin" />
        Restarting…
      </Button>
    );
  }

  // idle / checking / error → manual check
  return (
    <Button
      variant="ghost"
      size="iconSm"
      title="Check for updates"
      disabled={status === "checking"}
      onClick={() => void check(false)}
    >
      <RefreshCw className={status === "checking" ? "animate-spin" : undefined} />
    </Button>
  );
}
