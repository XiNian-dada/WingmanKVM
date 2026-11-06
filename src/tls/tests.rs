use super::*;
use crate::{
    client_ip::ClientIpListener,
    config::TlsConfig,
    proxy_protocol::{ProxyProtocolListener, ProxyProtocolMode},
};
use axum::serve::Listener;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

pub(crate) struct Directory(pub PathBuf);
impl Directory {
    pub(crate) fn new() -> Self {
        let mut random = [0_u8; 8];
        getrandom::fill(&mut random).unwrap();
        let path = std::env::temp_dir().join(format!(
            "wk-tls-{}-{:016x}",
            std::process::id(),
            u64::from_ne_bytes(random)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

pub(crate) fn pem_pair(server: &ServerConfig) -> (String, String) {
    let pem = generate_self_signed(server).unwrap();
    let cert = X509::stack_from_pem(&pem).unwrap().remove(0);
    let key = PKey::private_key_from_pem(&pem).unwrap();
    (
        String::from_utf8(cert.to_pem().unwrap()).unwrap(),
        String::from_utf8(key.private_key_to_pem_pkcs8().unwrap()).unwrap(),
    )
}

#[test]
fn expired_custom_certificate_is_rejected_without_writing_it() {
    let directory = Directory::new();
    let manager = TlsManager::new(&directory.0);
    let mut server = ServerConfig::default();
    server.public_origin = Some("https://localhost:38319".into());
    let (_, key_pem) = pem_pair(&server);
    let key = PKey::private_key_from_pem(key_pem.as_bytes()).unwrap();
    let mut name = X509NameBuilder::new().unwrap();
    name.append_entry_by_nid(Nid::COMMONNAME, "localhost")
        .unwrap();
    let name = name.build();
    let mut cert = X509::builder().unwrap();
    cert.set_version(2).unwrap();
    cert.set_subject_name(&name).unwrap();
    cert.set_issuer_name(&name).unwrap();
    cert.set_pubkey(&key).unwrap();
    let start = Asn1Time::from_unix(1).unwrap();
    let end = Asn1Time::from_unix(2).unwrap();
    cert.set_not_before(&start).unwrap();
    cert.set_not_after(&end).unwrap();
    cert.sign(&key, MessageDigest::sha256()).unwrap();
    let pem = String::from_utf8(cert.build().to_pem().unwrap()).unwrap();
    let error = manager.import(&pem, &key_pem, &server).unwrap_err();
    assert!(error.to_string().contains("过期"));
    assert!(!directory.0.join("tls").exists());
}

#[test]
fn self_signed_identity_is_private_unique_and_persistent() {
    let first = Directory::new();
    let second = Directory::new();
    let server = ServerConfig::default();
    let manager = TlsManager::new(&first.0);
    let one = manager.prepare(&server).unwrap().unwrap();
    let restart = TlsManager::new(&first.0).prepare(&server).unwrap().unwrap();
    let other = TlsManager::new(&second.0)
        .prepare(&server)
        .unwrap()
        .unwrap();
    assert_eq!(
        one.certificate.fingerprint_sha256,
        restart.certificate.fingerprint_sha256
    );
    assert_ne!(
        one.certificate.fingerprint_sha256,
        other.certificate.fingerprint_sha256
    );
    assert_eq!(
        fs::metadata(first.0.join("tls/self-signed.pem"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o600
    );
    assert_eq!(
        fs::metadata(first.0.join("tls"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o700
    );
    assert!(!one.certificate_pem.contains("PRIVATE KEY"));
    assert!(one.certificate.expires_in_days >= 3649);
    assert!(covers_host(&one.certificate.names, "localhost"));
}

#[test]
fn custom_import_validates_key_names_limits_and_preserves_active_identity() {
    let directory = Directory::new();
    let manager = TlsManager::new(&directory.0);
    let mut server = ServerConfig::default();
    server.public_origin = Some("https://kvm.example.com:38319".into());
    let original = manager.prepare(&server).unwrap();
    manager.activate(original.clone());
    let bundle = generate_self_signed(&server).unwrap();
    let key = String::from_utf8(
        PKey::private_key_from_pem(&bundle)
            .unwrap()
            .private_key_to_pem_pkcs8()
            .unwrap(),
    )
    .unwrap();
    let certificate =
        String::from_utf8(X509::stack_from_pem(&bundle).unwrap()[0].to_pem().unwrap()).unwrap();
    let different = generate_self_signed(&server).unwrap();
    let wrong_key = String::from_utf8(
        PKey::private_key_from_pem(&different)
            .unwrap()
            .private_key_to_pem_pkcs8()
            .unwrap(),
    )
    .unwrap();
    assert!(
        manager
            .import(&certificate, &wrong_key, &server)
            .unwrap_err()
            .to_string()
            .contains("不匹配")
    );
    let mut wrong_host = server.clone();
    wrong_host.public_origin = Some("https://other.example.com:38319".into());
    assert!(
        manager
            .import(&certificate, &key, &wrong_host)
            .unwrap_err()
            .to_string()
            .contains("域名")
    );
    assert!(manager.import("invalid", &key, &server).is_err());
    assert!(
        manager
            .import(&"a".repeat(128 * 1024 + 1), &key, &server)
            .is_err()
    );
    assert_eq!(fs::read_dir(directory.0.join("tls")).unwrap().count(), 1);
    let id = manager.import(&certificate, &key, &server).unwrap();
    assert_eq!(
        manager.current().unwrap().certificate.fingerprint_sha256,
        original.unwrap().certificate.fingerprint_sha256
    );
    server.tls.custom_certificate = Some(id);
    server.tls.mode = TlsMode::Custom;
    let custom = manager.prepare(&server).unwrap();
    let fingerprint = custom
        .as_ref()
        .unwrap()
        .certificate
        .fingerprint_sha256
        .clone();
    manager.activate(custom);
    server.tls.mode = TlsMode::SelfSigned;
    manager.activate(manager.prepare(&server).unwrap());
    server.tls.mode = TlsMode::Custom;
    manager.activate(manager.prepare(&server).unwrap());
    assert_eq!(
        manager.current().unwrap().certificate.fingerprint_sha256,
        fingerprint
    );
}

#[test]
fn certificate_paths_cannot_escape_or_follow_symlinks() {
    let directory = Directory::new();
    let manager = TlsManager::new(&directory.0);
    let mut server = ServerConfig::default();
    server.tls.self_signed_certificate = Some("../../auth.json".into());
    assert!(manager.prepare(&server).is_err());
    manager.ensure_directory().unwrap();
    fs::write(directory.0.join("other"), b"untouched").unwrap();
    std::os::unix::fs::symlink(
        directory.0.join("other"),
        directory.0.join("tls/self-signed.pem"),
    )
    .unwrap();
    server.tls.self_signed_certificate = None;
    assert!(manager.prepare(&server).is_err());
    assert_eq!(fs::read(directory.0.join("other")).unwrap(), b"untouched");
}

#[test]
fn default_https_and_legacy_http_deserialize_differently() {
    assert_eq!(ServerConfig::default().tls.mode, TlsMode::SelfSigned);
    let legacy: ServerConfig = serde_json::from_str(r#"{"port":8080}"#).unwrap();
    assert_eq!(legacy.tls.mode, TlsMode::Disabled);
    assert!(!legacy.session_cookie_secure());
    assert!(ServerConfig::default().session_cookie_secure());
    let invalid = TlsConfig {
        mode: TlsMode::Custom,
        ..TlsConfig::default()
    };
    assert!(invalid.validate().is_err());
}

#[test]
fn wildcard_certificate_matches_exactly_one_dns_label() {
    let names = vec!["*.example.com".to_owned(), "127.0.0.1".to_owned()];
    assert!(covers_host(&names, "kvm.example.com"));
    assert!(!covers_host(&names, "example.com"));
    assert!(!covers_host(&names, "a.kvm.example.com"));
    assert!(covers_host(&names, "127.0.0.1"));
    assert!(!covers_host(&names, "127.0.0.2"));
}

#[tokio::test]
async fn tls_after_proxy_protocol_preserves_client_ip_and_does_not_block_on_stalled_peers() {
    let directory = Directory::new();
    let manager = TlsManager::new(&directory.0);
    let identity = manager.prepare(&ServerConfig::default()).unwrap().unwrap();
    manager.activate(Some(identity.clone()));
    let tcp = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = tcp.local_addr().unwrap();
    let proxy = ProxyProtocolListener::new(tcp, ProxyProtocolMode::Auto);
    let mut listener = TlsListener::new(ClientIpListener(proxy), manager).unwrap();
    let mut stalled = tokio::net::TcpStream::connect(address).await.unwrap();
    stalled.write_all(&[0x16]).await.unwrap();
    let mut roots = rustls::RootCertStore::empty();
    let cert = X509::stack_from_pem(identity.certificate_pem.as_bytes()).unwrap();
    roots
        .add(CertificateDer::from(cert[0].to_der().unwrap()))
        .unwrap();
    let config = rustls::ClientConfig::builder_with_provider(Arc::new(
        rustls::crypto::ring::default_provider(),
    ))
    .with_safe_default_protocol_versions()
    .unwrap()
    .with_root_certificates(roots)
    .with_no_client_auth();
    let client = tokio::spawn(async move {
        let mut socket = tokio::net::TcpStream::connect(address).await.unwrap();
        let mut header = b"\r\n\r\n\0\r\nQUIT\n".to_vec();
        header.extend([0x21, 0x11, 0, 12]);
        header.extend([203, 0, 113, 9, 127, 0, 0, 1, 0x13, 0x88, 0x1f, 0x90]);
        socket.write_all(&header).await.unwrap();
        let mut stream = tokio_rustls::TlsConnector::from(Arc::new(config))
            .connect("localhost".try_into().unwrap(), socket)
            .await
            .unwrap();
        stream.write_all(b"hello").await.unwrap();
        let mut reply = [0; 2];
        stream.read_exact(&mut reply).await.unwrap();
        assert_eq!(&reply, b"ok");
    });
    let (mut stream, connection) =
        tokio::time::timeout(std::time::Duration::from_secs(2), listener.accept())
            .await
            .unwrap();
    assert!(connection.tls);
    assert_eq!(
        connection.client_addr,
        "203.0.113.9:5000".parse::<std::net::SocketAddr>().unwrap()
    );
    assert!(connection.transport_peer.unwrap().ip().is_loopback());
    let mut bytes = [0; 5];
    stream.read_exact(&mut bytes).await.unwrap();
    assert_eq!(&bytes, b"hello");
    stream.write_all(b"ok").await.unwrap();
    client.await.unwrap();
    drop(stalled);
}
