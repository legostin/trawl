use std::net::{IpAddr, UdpSocket};

/// Определяет локальный IP, используемый для исходящих соединений в LAN.
/// UDP `connect` не отправляет пакетов — только выбирает маршрут/интерфейс.
pub fn lan_ip() -> Option<IpAddr> {
    let sock = UdpSocket::bind("0.0.0.0:0").ok()?;
    sock.connect("8.8.8.8:80").ok()?;
    sock.local_addr().ok().map(|a| a.ip())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn returns_a_private_or_real_ip() {
        // На машине с сетью должен вернуться какой-то не-loopback IP.
        // Тест не должен падать при отсутствии сети — тогда допустим None.
        if let Some(ip) = lan_ip() {
            assert!(!ip.is_loopback(), "ожидали не-loopback, получили {ip}");
        }
    }
}
