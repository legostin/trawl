use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use rcgen::{BasicConstraints, CertificateParams, DistinguishedName, DnType, IsCa, KeyPair};

pub struct CaMaterial {
    pub key_pair: KeyPair,
    pub ca_cert: rcgen::Certificate,
    /// Публичный сертификат в PEM — раздаётся клиентам для установки.
    pub cert_pem: String,
}

/// Загружает CA из `dir/ca.key` + `dir/ca.pem`, либо создаёт новый и сохраняет его.
pub fn load_or_create_ca(dir: &Path) -> Result<CaMaterial> {
    let key_path = dir.join("ca.key");
    let cert_path = dir.join("ca.pem");

    if key_path.exists() && cert_path.exists() {
        let key_pem = fs::read_to_string(&key_path).context("read ca.key")?;
        let cert_pem = fs::read_to_string(&cert_path).context("read ca.pem")?;
        let key_pair = KeyPair::from_pem(&key_pem).context("parse ca.key")?;
        let ca_cert = CertificateParams::from_ca_cert_pem(&cert_pem)
            .context("parse ca.pem")?
            .self_signed(&key_pair)
            .context("re-sign ca")?;
        return Ok(CaMaterial { key_pair, ca_cert, cert_pem });
    }

    let (key_pair, ca_cert) = generate_ca()?;
    let cert_pem = ca_cert.pem();
    let key_pem = key_pair.serialize_pem();

    fs::create_dir_all(dir).context("create ca dir")?;
    fs::write(&cert_path, &cert_pem).context("write ca.pem")?;
    fs::write(&key_path, &key_pem).context("write ca.key")?;
    set_key_permissions(&key_path)?;

    Ok(CaMaterial { key_pair, ca_cert, cert_pem })
}

fn generate_ca() -> Result<(KeyPair, rcgen::Certificate)> {
    let mut params = CertificateParams::default();
    let mut dn = DistinguishedName::new();
    dn.push(DnType::CommonName, "Trawl CA");
    dn.push(DnType::OrganizationName, "Trawl");
    params.distinguished_name = dn;
    params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);

    let key_pair = KeyPair::generate()?;
    let cert = params.self_signed(&key_pair)?;
    Ok((key_pair, cert))
}

#[cfg(unix)]
fn set_key_permissions(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let mut perms = fs::metadata(path)?.permissions();
    perms.set_mode(0o600);
    fs::set_permissions(path, perms)?;
    Ok(())
}

#[cfg(not(unix))]
fn set_key_permissions(_path: &Path) -> Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_ca_files_when_absent() {
        let tmp = std::env::temp_dir().join(format!("httpcatch-ca-test-a-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        let mat = load_or_create_ca(&tmp).unwrap();
        assert!(mat.cert_pem.contains("BEGIN CERTIFICATE"));
        assert!(tmp.join("ca.key").exists());
        assert!(tmp.join("ca.pem").exists());
        std::fs::remove_dir_all(&tmp).unwrap();
    }

    #[test]
    fn reuses_existing_ca_on_second_call() {
        let tmp = std::env::temp_dir().join(format!("httpcatch-ca-test-b-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        let first = load_or_create_ca(&tmp).unwrap().cert_pem;
        let second = load_or_create_ca(&tmp).unwrap().cert_pem;
        assert_eq!(first, second, "CA должен переиспользоваться, а не пересоздаваться");
        std::fs::remove_dir_all(&tmp).unwrap();
    }
}
