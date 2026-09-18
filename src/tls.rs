use axum_server::tls_rustls::RustlsConfig;
use rcgen::{
    BasicConstraints, CertificateParams, DnType, ExtendedKeyUsagePurpose, IsCa, Issuer, KeyPair,
    KeyUsagePurpose,
};
use std::net::IpAddr;
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use tracing::info;

const CA_CERT_FILE: &str = "remotemic-ca.crt";
const CA_KEY_FILE: &str = "remotemic-ca.key";

pub struct LocalTls {
    pub config: RustlsConfig,
    pub ca_der: Vec<u8>,
    pub ca_path: PathBuf,
}

pub async fn prepare(directory: &Path, server_ips: &[IpAddr]) -> Result<LocalTls, String> {
    let ca_path = directory.join(CA_CERT_FILE);
    let ca_key_path = directory.join(CA_KEY_FILE);
    let (ca_pem, ca_key_pem) = load_or_create_ca(&ca_path, &ca_key_path)?;

    let ca_key = KeyPair::from_pem(&ca_key_pem)
        .map_err(|error| format!("Could not parse local CA key: {error}"))?;
    let issuer = Issuer::from_ca_cert_pem(&ca_pem, ca_key)
        .map_err(|error| format!("Could not parse local CA certificate: {error}"))?;

    let mut names = vec!["localhost".to_string()];
    for ip in server_ips {
        let name = ip.to_string();
        if !names.contains(&name) {
            names.push(name);
        }
    }
    let mut params = CertificateParams::new(names)
        .map_err(|error| format!("Could not create TLS certificate parameters: {error}"))?;
    params
        .distinguished_name
        .push(DnType::CommonName, "RemoteMic LAN server");
    params.use_authority_key_identifier_extension = true;
    params.key_usages = vec![KeyUsagePurpose::DigitalSignature];
    params.extended_key_usages = vec![ExtendedKeyUsagePurpose::ServerAuth];

    let server_key = KeyPair::generate()
        .map_err(|error| format!("Could not generate TLS server key: {error}"))?;
    let server_cert = params
        .signed_by(&server_key, &issuer)
        .map_err(|error| format!("Could not sign TLS server certificate: {error}"))?;

    let config = RustlsConfig::from_pem(
        server_cert.pem().into_bytes(),
        server_key.serialize_pem().into_bytes(),
    )
    .await
    .map_err(|error| format!("Could not configure HTTPS: {error}"))?;
    let ca_der = pem_certificate_to_der(&ca_pem)?;

    Ok(LocalTls {
        config,
        ca_der,
        ca_path,
    })
}

fn load_or_create_ca(cert_path: &Path, key_path: &Path) -> Result<(String, String), String> {
    match (
        std::fs::read_to_string(cert_path),
        std::fs::read_to_string(key_path),
    ) {
        (Ok(cert), Ok(key)) => return Ok((cert, key)),
        (Err(error), _) if error.kind() != std::io::ErrorKind::NotFound => {
            return Err(format!("Could not read {}: {error}", cert_path.display()));
        }
        (_, Err(error)) if error.kind() != std::io::ErrorKind::NotFound => {
            return Err(format!("Could not read {}: {error}", key_path.display()));
        }
        _ => {}
    }

    let key =
        KeyPair::generate().map_err(|error| format!("Could not generate local CA key: {error}"))?;
    let mut params = CertificateParams::default();
    params
        .distinguished_name
        .push(DnType::CommonName, "RemoteMic Local CA");
    params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
    params.key_usages = vec![
        KeyUsagePurpose::KeyCertSign,
        KeyUsagePurpose::DigitalSignature,
        KeyUsagePurpose::CrlSign,
    ];
    let cert = params
        .self_signed(&key)
        .map_err(|error| format!("Could not create local CA: {error}"))?;
    let cert_pem = cert.pem();
    let key_pem = key.serialize_pem();

    write_private(cert_path, cert_pem.as_bytes())?;
    write_private(key_path, key_pem.as_bytes())?;
    info!(ca = %cert_path.display(), "Created persistent RemoteMic local CA");
    Ok((cert_pem, key_pem))
}

fn write_private(path: &Path, data: &[u8]) -> Result<(), String> {
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true).mode(0o600);
    let mut file = options
        .open(path)
        .map_err(|error| format!("Could not create {}: {error}", path.display()))?;
    std::io::Write::write_all(&mut file, data)
        .map_err(|error| format!("Could not write {}: {error}", path.display()))
}

fn pem_certificate_to_der(pem: &str) -> Result<Vec<u8>, String> {
    let body = pem
        .lines()
        .filter(|line| !line.starts_with("-----"))
        .collect::<String>();
    use base64::Engine;
    base64::engine::general_purpose::STANDARD
        .decode(body)
        .map_err(|error| format!("Could not encode CA download: {error}"))
}

#[cfg(test)]
mod tests {
    use super::prepare;
    use std::net::{IpAddr, Ipv4Addr};

    #[tokio::test]
    async fn prepare_reuses_persistent_ca_for_new_server_certificates() {
        let directory = std::env::temp_dir().join(format!(
            "remotemic-tls-test-{}-{}",
            std::process::id(),
            rand::random::<u64>()
        ));
        std::fs::create_dir(&directory).unwrap();

        let first = prepare(&directory, &[IpAddr::V4(Ipv4Addr::new(192, 0, 2, 10))])
            .await
            .unwrap();
        let second = prepare(&directory, &[IpAddr::V4(Ipv4Addr::new(192, 0, 2, 11))])
            .await
            .unwrap();
        std::fs::remove_dir_all(directory).unwrap();

        assert_eq!(first.ca_der, second.ca_der);
    }
}
