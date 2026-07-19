import { useEffect, useState, type ReactNode } from "react";
import QRCode from "qrcode";
import {
  AlertTriangle,
  CheckCircle2,
  Download,
  Loader2,
  Settings,
  ShieldCheck,
  Wifi,
} from "lucide-react";
import { useFlows } from "../store";
import { getSetupInfo, caCertPath, type SetupInfo } from "../setup";
import { cn } from "@/lib/utils";

export function SetupPanel() {
  const [info, setInfo] = useState<SetupInfo | null>(null);
  const [certPath, setCertPath] = useState<string>("");
  const [qr, setQr] = useState<string>("");
  const running = useFlows((s) => s.running);
  const httpsSeen = useFlows((s) => s.flows.some((f) => f.url.scheme === "https"));

  useEffect(() => {
    getSetupInfo().then(setInfo).catch(() => {});
    caCertPath().then(setCertPath).catch(() => {});
    QRCode.toDataURL("http://http-catch/", { margin: 1, width: 320 })
      .then(setQr)
      .catch(() => setQr(""));
  }, []);

  const ip = info?.lanIp ?? "<no network>";
  const port = info?.port ?? 8888;

  return (
    <div className="mx-auto h-full max-w-2xl overflow-auto p-6">
      <h2 className="mb-1 text-lg font-semibold">Capture phone traffic</h2>
      <p className="mb-4 text-sm text-muted-foreground">
        Four steps and the device’s HTTPS traffic shows up in the Traffic tab.
      </p>

      <div
        className={cn(
          "mb-4 flex items-center gap-2 rounded-lg border px-3 py-2 text-sm",
          running
            ? "border-http-green/40 bg-http-green/10 text-foreground"
            : "border-http-amber/40 bg-http-amber/10 text-foreground",
        )}
      >
        {running ? (
          <CheckCircle2 className="size-4 text-http-green" />
        ) : (
          <AlertTriangle className="size-4 text-http-amber" />
        )}
        {running ? (
          <span>Proxy running on {info ? `${ip}:${port}` : `port ${port}`}.</span>
        ) : (
          <span>
            Press <b>Start</b> in the top bar first — otherwise the proxy is not running and the phone cannot connect.
          </span>
        )}
      </div>

      <Step n={1} icon={<Wifi className="size-4" />} title="Same Wi-Fi network + proxy address">
        <p>The phone and this Mac must be on the same network. Proxy address:</p>
        <div className="mt-2 inline-block rounded-md bg-secondary px-3 py-1.5 font-mono text-base font-semibold">
          {ip}:{port}
        </div>
      </Step>

      <Step n={2} icon={<Settings className="size-4" />} title="Configure the proxy on the phone">
        <p>
          Wi-Fi → network settings → HTTP proxy <b>manual</b> → enter IP <code>{ip}</code> and port{" "}
          <code>{port}</code>.
        </p>
      </Step>

      <Step n={3} icon={<Download className="size-4" />} title="Download the CA certificate">
        <p>
          On the phone open <code>http://http-catch/</code> (scan the QR) — the certificate
          downloads. On iOS open it <b>in Safari specifically</b>.
        </p>
        {qr && (
          <img
            src={qr}
            width={150}
            height={150}
            alt="QR http://http-catch/"
            className="mt-3 rounded-md bg-white p-1"
          />
        )}
        {certPath && (
          <p className="mt-2 text-xs text-muted-foreground">
            File on disk: <code className="break-all">{certPath}</code>
          </p>
        )}
      </Step>

      <Step n={4} icon={<ShieldCheck className="size-4" />} title="Install and trust">
        <p className="font-medium">iOS — three separate steps:</p>
        <ol className="mt-1 list-decimal space-y-0.5 pl-5">
          <li>In Safari tap “Allow” to download the profile.</li>
          <li>
            Settings → General → <b>VPN &amp; Device Management</b> → the <b>http-catch CA</b> profile →{" "}
            <b>Install</b>.
          </li>
          <li>
            Settings → General → About → <b>Certificate Trust Settings</b> → enable the toggle.
          </li>
        </ol>
        <p className="mt-2 font-medium">Android:</p>
        <p>Settings → Security → Encryption &amp; credentials → Install a certificate → CA certificate.</p>
      </Step>

      <div
        className={cn(
          "mt-4 flex items-center gap-2 rounded-lg border px-3 py-2.5 text-sm",
          httpsSeen
            ? "border-http-green/40 bg-http-green/10"
            : "border-border bg-muted/40 text-muted-foreground",
        )}
      >
        {httpsSeen ? (
          <CheckCircle2 className="size-4 text-http-green" />
        ) : (
          <Loader2 className="size-4 animate-spin" />
        )}
        {httpsSeen
          ? "HTTPS traffic is being decrypted — all set."
          : "Waiting for the first decrypted HTTPS request…"}
      </div>
    </div>
  );
}

function Step({
  n,
  icon,
  title,
  children,
}: {
  n: number;
  icon: ReactNode;
  title: string;
  children: ReactNode;
}) {
  return (
    <div className="mb-3 rounded-lg border border-border bg-card p-4">
      <div className="mb-1.5 flex items-center gap-2">
        <span className="flex size-6 items-center justify-center rounded-full bg-primary text-xs font-bold text-primary-foreground">
          {n}
        </span>
        <span className="text-primary">{icon}</span>
        <h3 className="text-sm font-semibold">{title}</h3>
      </div>
      <div className="pl-8 text-sm text-muted-foreground [&_code]:rounded [&_code]:bg-secondary [&_code]:px-1 [&_code]:font-mono [&_code]:text-foreground">
        {children}
      </div>
    </div>
  );
}
