use anyhow::Result;
use rcgen::{BasicConstraints, CertificateParams, DistinguishedName, DnType, IsCa, KeyPair};

/// Генерирует самоподписанный CA в памяти. В Phase 1 нужен только чтобы hudsucker стартовал;
/// HTTPS-перехват (доверие на устройстве) добавляется в Phase 2.
pub fn generate_ephemeral_ca() -> Result<(KeyPair, rcgen::Certificate)> {
    let mut params = CertificateParams::default();
    let mut dn = DistinguishedName::new();
    dn.push(DnType::CommonName, "http-catch CA");
    dn.push(DnType::OrganizationName, "http-catch");
    params.distinguished_name = dn;
    params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);

    let key_pair = KeyPair::generate()?;
    let cert = params.self_signed(&key_pair)?;
    Ok((key_pair, cert))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generates_a_ca_certificate() {
        let (_kp, cert) = generate_ephemeral_ca().unwrap();
        let pem = cert.pem();
        assert!(pem.contains("BEGIN CERTIFICATE"));
    }
}
