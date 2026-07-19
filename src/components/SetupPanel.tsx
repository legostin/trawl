import { useEffect, useState } from "react";
import QRCode from "qrcode";
import { useFlows } from "../store";
import { getSetupInfo, caCertPath, type SetupInfo } from "../setup";

export function SetupPanel() {
  const [info, setInfo] = useState<SetupInfo | null>(null);
  const [certPath, setCertPath] = useState<string>("");
  const [qr, setQr] = useState<string>("");
  const httpsSeen = useFlows((s) => s.flows.some((f) => f.url.scheme === "https"));

  useEffect(() => {
    getSetupInfo().then(setInfo);
    caCertPath().then(setCertPath);
    QRCode.toDataURL("http://http-catch/")
      .then(setQr)
      .catch(() => setQr(""));
  }, []);

  const ip = info?.lanIp ?? "<нет сети>";
  const port = info?.port ?? 8888;

  return (
    <div style={{ padding: 16, overflow: "auto", height: "100%", fontSize: 13, lineHeight: 1.5 }}>
      <h2 style={{ marginTop: 0 }}>Настройка перехвата трафика телефона</h2>

      <div
        style={{
          marginBottom: 12,
          padding: 10,
          borderRadius: 6,
          background: "#3a2a1e",
          border: "1px solid #6a4a2a",
        }}
      >
        <b>Сначала нажмите «Start proxy»</b> в шапке — до этого прокси не запущен и телефон
        не сможет подключиться.
      </div>

      <ol style={{ paddingLeft: 20 }}>
        <li>
          Телефон и этот Mac должны быть в одной Wi-Fi-сети. Адрес прокси:{" "}
          <code style={{ fontSize: 15, background: "#333", padding: "2px 6px" }}>
            {ip}:{port}
          </code>
        </li>
        <li>
          На телефоне: Wi-Fi → настройки сети → HTTP-прокси <b>вручную</b> → впишите IP{" "}
          <code>{ip}</code> и порт <code>{port}</code>.
        </li>
        <li>
          Скачайте CA-сертификат. На телефоне откройте <code>http://http-catch/</code>{" "}
          (отсканируйте QR) — сертификат скачается. На iOS открывайте <b>именно в Safari</b>:
          другие браузеры (Chrome и пр.) не умеют устанавливать профили.
          <div style={{ marginTop: 8 }}>
            {qr && <img src={qr} width={160} height={160} alt="QR http://http-catch/" />}
          </div>
          <div style={{ opacity: 0.75, marginTop: 4 }}>
            Файл сертификата на диске: <code>{certPath}</code>
          </div>
        </li>
        <li>
          <b>Установите и доверьте</b> сертификат вручную:
          <div style={{ marginTop: 4 }}>
            <b>iOS</b> — три отдельных шага (пропуск любого = сертификат «не виден»):
            <ol style={{ paddingLeft: 20, marginTop: 2 }}>
              <li>В Safari нажмите «Allow» на запрос загрузки конфигурационного профиля.</li>
              <li>
                Settings → General → <b>VPN &amp; Device Management</b> (на старых iOS — «Profile
                Downloaded» вверху Settings) → выберите профиль <b>http-catch CA</b> → <b>Install</b>{" "}
                (введите пасскод).
              </li>
              <li>
                Settings → General → About → <b>Certificate Trust Settings</b> → включите тумблер
                для http-catch CA.
              </li>
            </ol>
          </div>
          <div style={{ marginTop: 4 }}>
            <b>Android</b>: Settings → Security → Encryption &amp; credentials → Install a certificate
            → <b>CA certificate</b> → выберите скачанный файл.
          </div>
        </li>
      </ol>

      <div
        style={{
          marginTop: 12,
          padding: 10,
          borderRadius: 6,
          background: httpsSeen ? "#1e4d2b" : "#3a3a1e",
        }}
      >
        {httpsSeen
          ? "✓ HTTPS-трафик расшифровывается — всё работает."
          : "Ожидание первого расшифрованного HTTPS-запроса…"}
      </div>
    </div>
  );
}
