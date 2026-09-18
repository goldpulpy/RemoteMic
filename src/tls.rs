use axum_server::tls_rustls::RustlsConfig;
use rcgen::{
    BasicConstraints, CertificateParams, DnType, ExtendedKeyUsagePurpose, IsCa, Issuer, KeyPair,
    KeyUsagePurpose,
};
use std::ffi::OsStr;
use std::net::IpAddr;
use std::os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt};
use std::path::{Path, PathBuf};
use tracing::info;

const CA_CERT_FILE: &str = "remotemic-ca.crt";
const CA_KEY_FILE: &str = "remotemic-ca.key";
const CA_TRANSACTION_FILE: &str = ".remotemic-ca-creating";

pub fn certificate_directory() -> Result<PathBuf, String> {
    if let Some(directory) = std::env::var_os("XDG_DATA_HOME").filter(|value| !value.is_empty()) {
        return Ok(PathBuf::from(directory).join("remotemic"));
    }
    certificate_directory_from(std::env::var_os("HOME").as_deref()).ok_or_else(|| {
        "Could not determine persistent certificate directory: HOME is unset".to_string()
    })
}

fn certificate_directory_from(home: Option<&OsStr>) -> Option<PathBuf> {
    home.filter(|directory| !directory.is_empty())
        .map(|directory| PathBuf::from(directory).join(".local/share/remotemic"))
}

pub struct LocalTls {
    pub config: RustlsConfig,
    pub ca_der: Vec<u8>,
    pub ca_path: PathBuf,
}

pub async fn prepare(directory: &Path, server_ips: &[IpAddr]) -> Result<LocalTls, String> {
    ensure_private_directory(directory)?;
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

fn ensure_private_directory(directory: &Path) -> Result<(), String> {
    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(directory)
        .map_err(|error| format!("Could not create {}: {error}", directory.display()))?;

    let metadata = std::fs::symlink_metadata(directory)
        .map_err(|error| format!("Could not inspect {}: {error}", directory.display()))?;
    if !metadata.file_type().is_dir() {
        return Err(format!("{} is not a directory", directory.display()));
    }

    let uid = std::fs::metadata("/proc/self")
        .map_err(|error| format!("Could not determine the current user: {error}"))?
        .uid();
    if metadata.uid() != uid {
        return Err(format!(
            "Unsafe certificate directory {}: owned by uid {}, expected uid {uid}",
            directory.display(),
            metadata.uid()
        ));
    }

    let permissions = metadata.mode() & 0o777;
    if permissions != 0o700 {
        return Err(format!(
            "Unsafe certificate directory {}: permissions are {permissions:#o}, expected 0o700",
            directory.display()
        ));
    }

    Ok(())
}

fn load_or_create_ca(cert_path: &Path, key_path: &Path) -> Result<(String, String), String> {
    recover_interrupted_ca_creation(cert_path, key_path)?;
    let cert = read_optional(cert_path)?;
    let key = read_optional(key_path)?;
    match (cert, key) {
        (Some(cert), Some(key)) => Ok((cert, key)),
        (None, None) => create_ca(cert_path, key_path),
        (Some(_), None) => Err(format!(
            "Local CA certificate {} exists but its private key {} is missing; \
             remove the directory and reinstall the CA on every device",
            cert_path.display(),
            key_path.display()
        )),
        (None, Some(_)) => Err(format!(
            "Local CA private key {} exists but its certificate {} is missing; \
             remove the directory and reinstall the CA on every device",
            key_path.display(),
            cert_path.display()
        )),
    }
}

fn read_optional(path: &Path) -> Result<Option<String>, String> {
    match std::fs::read_to_string(path) {
        Ok(value) => Ok(Some(value)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(format!("Could not read {}: {error}", path.display())),
    }
}

fn create_ca(cert_path: &Path, key_path: &Path) -> Result<(String, String), String> {
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

    let directory = cert_path
        .parent()
        .ok_or_else(|| format!("CA path {} has no parent directory", cert_path.display()))?;
    let transaction_path = directory.join(CA_TRANSACTION_FILE);
    write_private(&transaction_path, b"creating local CA\n")?;

    let result = (|| -> Result<(), String> {
        write_private(key_path, key_pem.as_bytes())?;
        write_private(cert_path, cert_pem.as_bytes())?;
        sync_directory(directory)?;
        std::fs::remove_file(&transaction_path).map_err(|error| {
            format!(
                "Could not finish local CA transaction {}: {error}",
                transaction_path.display()
            )
        })?;
        sync_directory(directory)
    })();
    result?;
    info!(ca = %cert_path.display(), "Created persistent RemoteMic local CA");
    Ok((cert_pem, key_pem))
}

fn recover_interrupted_ca_creation(cert_path: &Path, key_path: &Path) -> Result<(), String> {
    let Some(directory) = cert_path.parent() else {
        return Ok(());
    };
    let transaction_path = directory.join(CA_TRANSACTION_FILE);
    if !transaction_path.exists() {
        return Ok(());
    }

    for path in [cert_path, key_path, transaction_path.as_path()] {
        match std::fs::remove_file(path) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(format!(
                    "Could not recover interrupted local CA creation at {}: {error}",
                    path.display()
                ));
            }
        }
    }
    sync_directory(directory)
}

fn sync_directory(directory: &Path) -> Result<(), String> {
    std::fs::File::open(directory)
        .and_then(|file| file.sync_all())
        .map_err(|error| format!("Could not synchronize {}: {error}", directory.display()))
}

fn write_private(path: &Path, data: &[u8]) -> Result<(), String> {
    let temporary = path.with_extension(format!(
        "tmp-{}-{}",
        std::process::id(),
        rand::random::<u64>()
    ));
    let result = (|| -> std::io::Result<()> {
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true).mode(0o600);
        let mut file = options.open(&temporary)?;
        std::io::Write::write_all(&mut file, data)?;
        file.sync_all()?;
        std::fs::rename(&temporary, path)
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
    result.map_err(|error| format!("Could not write {}: {error}", path.display()))
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
    use super::{CA_TRANSACTION_FILE, certificate_directory_from, load_or_create_ca, prepare};
    use std::ffi::OsStr;
    use std::net::{IpAddr, Ipv4Addr};

    #[tokio::test]
    async fn prepare_reuses_persistent_ca_for_new_server_certificates() {
        let directory = std::env::temp_dir().join(format!(
            "remotemic-tls-test-{}-{}",
            std::process::id(),
            rand::random::<u64>()
        ));

        let first = prepare(&directory, &[IpAddr::V4(Ipv4Addr::new(192, 0, 2, 10))])
            .await
            .unwrap();
        let second = prepare(&directory, &[IpAddr::V4(Ipv4Addr::new(192, 0, 2, 11))])
            .await
            .unwrap();
        std::fs::remove_dir_all(directory).unwrap();

        assert_eq!(first.ca_der, second.ca_der);
    }

    #[test]
    fn load_or_create_ca_rejects_a_missing_private_key() {
        let directory = std::env::temp_dir().join(format!(
            "remotemic-tls-mismatch-{}-{}",
            std::process::id(),
            rand::random::<u64>()
        ));
        std::fs::create_dir_all(&directory).unwrap();
        let cert_path = directory.join("remotemic-ca.crt");
        let key_path = directory.join("remotemic-ca.key");
        std::fs::write(&cert_path, b"-----BEGIN CERTIFICATE-----\n").unwrap();

        let result = load_or_create_ca(&cert_path, &key_path);
        std::fs::remove_dir_all(directory).unwrap();

        assert!(result.is_err());
    }

    #[test]
    fn certificate_directory_uses_local_share_in_home() {
        let directory = certificate_directory_from(Some(OsStr::new("/home/example")));

        assert_eq!(
            directory,
            Some("/home/example/.local/share/remotemic".into())
        );
    }

    #[test]
    fn certificate_directory_is_unavailable_without_home() {
        assert_eq!(certificate_directory_from(None), None);
    }

    #[test]
    fn load_or_create_ca_recovers_an_interrupted_initial_creation() {
        let directory = std::env::temp_dir().join(format!(
            "remotemic-tls-recovery-{}-{}",
            std::process::id(),
            rand::random::<u64>()
        ));
        std::fs::create_dir_all(&directory).unwrap();
        let cert_path = directory.join("remotemic-ca.crt");
        let key_path = directory.join("remotemic-ca.key");
        std::fs::write(&key_path, b"incomplete key").unwrap();
        std::fs::write(directory.join(CA_TRANSACTION_FILE), b"creating").unwrap();

        let result = load_or_create_ca(&cert_path, &key_path);
        std::fs::remove_dir_all(directory).unwrap();

        assert!(result.is_ok(), "unexpected recovery error: {result:?}");
    }
}
