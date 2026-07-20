import { useEffect, useState, type ReactNode } from "react";
import QRCode from "qrcode";
import {
  AlertTriangle,
  CheckCircle2,
  Globe,
  Download,
  FolderSearch,
  Loader2,
  Power,
  PowerOff,
  Settings,
  ShieldCheck,
  Smartphone,
  Wifi,
} from "lucide-react";
import { useFlows } from "../store";
import { useToast } from "../toast";
import {
  getSetupInfo,
  caCertPath,
  revealCaCert,
  trustCaMacos,
  setSystemProxy,
  installCaIosSimulator,
  launchChromeProxy,
  type SetupInfo,
} from "../setup";
import { CopyableCommand } from "./CopyableCommand";
import { Button } from "./ui/button";
import { Segmented } from "./ui/segmented";
import { cn } from "@/lib/utils";

type Scenario = "phone" | "mac" | "chrome" | "ios" | "android";

export function SetupPanel() {
  const [scenario, setScenario] = useState<Scenario>("mac");
  const [info, setInfo] = useState<SetupInfo | null>(null);
  const [certPath, setCertPath] = useState("");
  const [qr, setQr] = useState("");
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
      <h2 className="mb-1 text-lg font-semibold">Setup — capture traffic</h2>
      <p className="mb-3 text-sm text-muted-foreground">
        Pick where you want to intercept traffic, then follow the steps.
      </p>

      <Segmented<Scenario>
        value={scenario}
        onChange={setScenario}
        className="mb-4"
        options={[
          { value: "mac", label: "This Mac" },
          { value: "chrome", label: "Chrome" },
          { value: "ios", label: "iOS Sim" },
          { value: "android", label: "Android" },
          { value: "phone", label: "Phone" },
        ]}
      />

      <div
        className={cn(
          "mb-4 flex items-center gap-2 rounded-lg border px-3 py-2 text-sm",
          running ? "border-http-green/40 bg-http-green/10" : "border-http-amber/40 bg-http-amber/10",
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
            Press <b>Start</b> in the top bar first — otherwise nothing is captured.
          </span>
        )}
      </div>

      {scenario === "mac" && <MacSteps certPath={certPath} />}
      {scenario === "chrome" && <ChromeSteps certPath={certPath} />}
      {scenario === "ios" && <IosSteps certPath={certPath} />}
      {scenario === "android" && <AndroidSteps />}
      {scenario === "phone" && <PhoneSteps ip={ip} port={port} qr={qr} certPath={certPath} />}

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

// ── scenarios ──

function MacSteps({ certPath }: { certPath: string }) {
  return (
    <>
      <Step n={1} icon={<ShieldCheck className="size-4" />} title="Trust the CA in the System keychain">
        <p>Adds http-catch’s root certificate as trusted (asks for your password).</p>
        <div className="mt-2 flex flex-wrap gap-1.5">
          <ActionButton icon={<ShieldCheck />} label="Trust CA" run={trustCaMacos} done="CA trusted" />
          <RevealButton />
        </div>
        <CertPath certPath={certPath} />
      </Step>
      <Step n={2} icon={<Settings className="size-4" />} title="Route the Mac through the proxy">
        <p>Sets the system HTTP/HTTPS proxy to 127.0.0.1:8888. Every app that honors it is captured.</p>
        <div className="mt-2 flex flex-wrap gap-1.5">
          <ActionButton icon={<Power />} label="Enable system proxy" run={() => setSystemProxy(true)} done="System proxy on" />
          <ActionButton icon={<PowerOff />} label="Disable" variant="ghost" run={() => setSystemProxy(false)} done="System proxy off" />
        </div>
      </Step>
    </>
  );
}

function ChromeSteps({ certPath }: { certPath: string }) {
  return (
    <>
      <Step n={1} icon={<ShieldCheck className="size-4" />} title="Trust the CA">
        <p>Chrome on macOS uses the system keychain for trust.</p>
        <div className="mt-2 flex flex-wrap gap-1.5">
          <ActionButton icon={<ShieldCheck />} label="Trust CA" run={trustCaMacos} done="CA trusted" />
          <RevealButton />
        </div>
        <CertPath certPath={certPath} />
      </Step>
      <Step n={2} icon={<Globe className="size-4" />} title="Launch Chrome through the proxy">
        <p>Opens a separate Chrome profile pointed at the proxy — your main Chrome stays untouched.</p>
        <div className="mt-2">
          <ActionButton icon={<Globe />} label="Launch Chrome via proxy" run={launchChromeProxy} done="Chrome launched" />
        </div>
      </Step>
    </>
  );
}

function IosSteps({ certPath }: { certPath: string }) {
  return (
    <>
      <Step n={1} icon={<Smartphone className="size-4" />} title="Boot a simulator">
        <p>Open the target simulator in Xcode / Simulator first (it must be booted).</p>
      </Step>
      <Step n={2} icon={<ShieldCheck className="size-4" />} title="Install the CA into the booted simulator">
        <div className="mt-1 flex flex-wrap gap-1.5">
          <ActionButton icon={<Download />} label="Install CA into simulator" run={installCaIosSimulator} done="CA installed in simulator" />
          <RevealButton />
        </div>
        <CertPath certPath={certPath} />
      </Step>
      <Step n={3} icon={<Settings className="size-4" />} title="Route through the proxy">
        <p>The simulator uses the Mac’s network, so enable the system proxy.</p>
        <div className="mt-2 flex flex-wrap gap-1.5">
          <ActionButton icon={<Power />} label="Enable system proxy" run={() => setSystemProxy(true)} done="System proxy on" />
          <ActionButton icon={<PowerOff />} label="Disable" variant="ghost" run={() => setSystemProxy(false)} done="System proxy off" />
        </div>
      </Step>
    </>
  );
}

function AndroidSteps() {
  return (
    <>
      <Step n={1} icon={<Settings className="size-4" />} title="Point the emulator at the proxy">
        <p>
          The emulator reaches the host at <code>10.0.2.2</code>. Launch it with the proxy flag, or set
          it in the emulator’s Wi-Fi settings (proxy <code>10.0.2.2:8888</code>).
        </p>
        <CopyableCommand cmd="emulator -avd <name> -http-proxy http://10.0.2.2:8888" />
      </Step>
      <Step n={2} icon={<Download className="size-4" />} title="Install the CA">
        <p>
          In the emulator browser open <code>http://http-catch/</code> and install it as a user
          certificate.
        </p>
        <p className="mt-1.5 text-http-amber">
          Note: on Android 7+ apps don’t trust user CAs by default. For full interception install it as
          a system CA on a writable-system emulator:
        </p>
        <CopyableCommand
          cmd={
            "adb root && adb remount\n" +
            "HASH=$(openssl x509 -inform PEM -subject_hash_old -in ca.pem | head -1)\n" +
            "adb push ca.pem /system/etc/security/cacerts/$HASH.0\n" +
            "adb reboot"
          }
        />
      </Step>
    </>
  );
}

function PhoneSteps({
  ip,
  port,
  qr,
  certPath,
}: {
  ip: string;
  port: number;
  qr: string;
  certPath: string;
}) {
  return (
    <>
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
          On the phone open <code>http://http-catch/</code> (scan the QR) — the certificate downloads.
          On iOS open it <b>in Safari specifically</b>.
        </p>
        {qr && (
          <img src={qr} width={150} height={150} alt="QR http://http-catch/" className="mt-3 rounded-md bg-white p-1" />
        )}
        <CertPath certPath={certPath} />
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
    </>
  );
}

// ── shared bits ──

function ActionButton({
  icon,
  label,
  run,
  done,
  variant = "outline",
}: {
  icon: ReactNode;
  label: string;
  run: () => Promise<void>;
  done: string;
  variant?: "outline" | "ghost";
}) {
  const show = useToast((s) => s.show);
  const onClick = async () => {
    try {
      await run();
      show(done);
    } catch (e) {
      show(`Error: ${String(e)}`);
    }
  };
  return (
    <Button size="sm" variant={variant} onClick={onClick}>
      {icon}
      {label}
    </Button>
  );
}

function RevealButton() {
  return (
    <ActionButton icon={<FolderSearch />} label="Reveal cert" variant="ghost" run={revealCaCert} done="Revealed in Finder" />
  );
}

function CertPath({ certPath }: { certPath: string }) {
  if (!certPath) return null;
  return (
    <p className="mt-2 text-xs text-muted-foreground">
      Certificate file: <code className="break-all">{certPath}</code>
    </p>
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
