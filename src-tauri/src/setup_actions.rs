use std::path::PathBuf;
use std::process::Command;

use tauri::{AppHandle, Manager};

/// Путь к публичному CA (создаёт при необходимости).
fn ca_pem_path(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join("ca");
    crate::ca::load_or_create_ca(&dir).map_err(|e| e.to_string())?;
    Ok(dir.join("ca.pem"))
}

/// Разбирает `networksetup -listnetworkserviceorder`: по устройству (enX) → имя сервиса.
pub fn parse_service_for_device(output: &str, device: &str) -> Option<String> {
    let mut current: Option<String> = None;
    for line in output.lines() {
        let l = line.trim();
        if let Some(rest) = l.strip_prefix('(') {
            // строка вида "(1) Wi-Fi"
            if let Some(idx) = rest.find(')') {
                let after = rest[idx + 1..].trim();
                if !after.is_empty() && !after.starts_with("Hardware Port:") {
                    current = Some(after.to_string());
                    continue;
                }
            }
            // строка "(Hardware Port: …, Device: en0)"
            if l.contains(&format!("Device: {device}")) {
                if let Some(svc) = current.clone() {
                    return Some(svc);
                }
            }
        }
    }
    None
}

/// Имя основного сетевого сервиса (по дефолтному маршруту), fallback "Wi-Fi".
fn primary_service() -> String {
    let device = Command::new("route")
        .args(["-n", "get", "default"])
        .output()
        .ok()
        .and_then(|o| {
            String::from_utf8_lossy(&o.stdout)
                .lines()
                .find_map(|l| l.trim().strip_prefix("interface:").map(|s| s.trim().to_string()))
        });
    if let Some(dev) = device {
        if let Ok(o) = Command::new("networksetup").arg("-listnetworkserviceorder").output() {
            if let Some(svc) = parse_service_for_device(&String::from_utf8_lossy(&o.stdout), &dev) {
                return svc;
            }
        }
    }
    "Wi-Fi".to_string()
}

fn run(mut cmd: Command) -> Result<(), String> {
    let out = cmd.output().map_err(|e| e.to_string())?;
    if out.status.success() {
        Ok(())
    } else {
        let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
        Err(if err.is_empty() { "command failed".into() } else { err })
    }
}

/// Выполняет shell-скрипт с правами администратора (нативный запрос пароля).
fn admin_shell(script: &str) -> Result<(), String> {
    let escaped = script.replace('\\', "\\\\").replace('"', "\\\"");
    let apple = format!("do shell script \"{escaped}\" with administrator privileges");
    let mut cmd = Command::new("osascript");
    cmd.arg("-e").arg(apple);
    run(cmd)
}

#[tauri::command]
pub fn reveal_ca_cert(app: AppHandle) -> Result<(), String> {
    let ca = ca_pem_path(&app)?;
    let mut cmd = Command::new("open");
    cmd.arg("-R").arg(&ca);
    run(cmd)
}

/// Команда для добавления CA в System keychain (для копирования / Terminal).
pub fn trust_command(ca: &std::path::Path) -> String {
    format!(
        "sudo security add-trusted-cert -d -r trustRoot -k /Library/Keychains/System.keychain '{}'",
        ca.display()
    )
}

#[tauri::command]
pub fn trust_ca_command(app: AppHandle) -> Result<String, String> {
    Ok(trust_command(&ca_pem_path(&app)?))
}

/// Открывает Terminal и запускает sudo-команду доверия. Установка доверенного корня
/// требует интерактивного сеанса (иначе SecTrustSettings падает «no user interaction»).
#[tauri::command]
pub fn trust_ca_macos(app: AppHandle) -> Result<(), String> {
    let ca = ca_pem_path(&app)?;
    let cmd = trust_command(&ca).replace('\\', "\\\\").replace('"', "\\\"");
    let apple = format!("tell application \"Terminal\"\nactivate\ndo script \"{cmd}\"\nend tell");
    let mut c = Command::new("osascript");
    c.arg("-e").arg(apple);
    run(c)
}

#[tauri::command]
pub fn set_system_proxy(enable: bool) -> Result<(), String> {
    let svc = primary_service();
    let script = if enable {
        format!(
            "networksetup -setwebproxy '{svc}' 127.0.0.1 8729; \
             networksetup -setsecurewebproxy '{svc}' 127.0.0.1 8729; \
             networksetup -setwebproxystate '{svc}' on; \
             networksetup -setsecurewebproxystate '{svc}' on"
        )
    } else {
        format!(
            "networksetup -setwebproxystate '{svc}' off; \
             networksetup -setsecurewebproxystate '{svc}' off"
        )
    };
    admin_shell(&script)
}

#[tauri::command]
pub fn system_proxy_enabled() -> bool {
    let svc = primary_service();
    Command::new("networksetup")
        .args(["-getsecurewebproxy", &svc])
        .output()
        .ok()
        .map(|o| {
            String::from_utf8_lossy(&o.stdout)
                .lines()
                .any(|l| l.trim().eq_ignore_ascii_case("Enabled: Yes"))
        })
        .unwrap_or(false)
}

#[tauri::command]
pub fn ios_simulator_booted() -> bool {
    Command::new("xcrun")
        .args(["simctl", "list", "devices", "booted"])
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).contains("(Booted)"))
        .unwrap_or(false)
}

#[tauri::command]
pub fn install_ca_ios_simulator(app: AppHandle) -> Result<(), String> {
    let ca = ca_pem_path(&app)?;
    let mut cmd = Command::new("xcrun");
    cmd.args(["simctl", "keychain", "booted", "add-root-cert"]).arg(&ca);
    run(cmd)
}

#[tauri::command]
pub fn launch_chrome_proxy(app: AppHandle) -> Result<(), String> {
    let profile = app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join("chrome-proxy");
    let mut cmd = Command::new("open");
    cmd.args(["-na", "Google Chrome", "--args", "--proxy-server=http://127.0.0.1:8729"])
        .arg(format!("--user-data-dir={}", profile.display()));
    run(cmd)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "An asterisk (*) denotes that a network service is disabled.\n\
(1) Wi-Fi\n\
(Hardware Port: Wi-Fi, Device: en0)\n\
\n\
(2) Thunderbolt Ethernet\n\
(Hardware Port: Thunderbolt Ethernet, Device: en1)\n";

    #[test]
    fn finds_service_by_device() {
        assert_eq!(parse_service_for_device(SAMPLE, "en0").as_deref(), Some("Wi-Fi"));
        assert_eq!(
            parse_service_for_device(SAMPLE, "en1").as_deref(),
            Some("Thunderbolt Ethernet")
        );
        assert_eq!(parse_service_for_device(SAMPLE, "en9"), None);
    }
}
