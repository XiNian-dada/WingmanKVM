//! Device-owned certificates. Immutable identity files make config publication atomic:
//! a failed settings write never replaces the certificate used on the next boot.
mod listener;
#[cfg(test)]
pub(crate) mod tests;
pub use listener::TlsListener;

use std::{
    cmp::Ordering,
    fs::{self, OpenOptions},
    io::{Read, Write},
    net::IpAddr,
    os::unix::fs::{OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
    sync::{Arc, RwLock},
};

use anyhow::{Context, Result, ensure};
use openssl::{
    asn1::Asn1Time,
    bn::{BigNum, MsbOption},
    ec::{EcGroup, EcKey},
    hash::MessageDigest,
    nid::Nid,
    pkey::PKey,
    x509::{
        X509, X509NameBuilder,
        extension::{BasicConstraints, ExtendedKeyUsage, KeyUsage, SubjectAlternativeName},
    },
};
use serde::Serialize;
use sha2::{Digest, Sha256};
use tokio_rustls::rustls::{
    self,
    pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer},
};

use crate::config::{NormalizedOrigin, ServerConfig, TlsMode};

pub const MAX_PEM_BYTES: usize = 192 * 1024;

#[derive(Clone, Debug, Serialize)]
pub struct CertificateStatus {
    pub subject: String,
    pub issuer: String,
    pub names: Vec<String>,
    pub not_after: String,
    pub expires_in_days: i32,
    pub fingerprint_sha256: String,
}

pub struct Identity {
    pub config: Arc<rustls::ServerConfig>,
    pub certificate: CertificateStatus,
    pub certificate_pem: String,
}

#[derive(Clone)]
pub struct TlsManager {
    directory: Arc<PathBuf>,
    active: Arc<RwLock<Option<Arc<Identity>>>>,
}

impl TlsManager {
    pub fn new(state_dir: &Path) -> Self {
        Self {
            directory: Arc::new(state_dir.join("tls")),
            active: Arc::new(RwLock::new(None)),
        }
    }

    pub fn enabled(&self) -> bool {
        self.current().is_some()
    }

    pub fn current(&self) -> Option<Arc<Identity>> {
        self.active
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    pub fn activate(&self, identity: Option<Arc<Identity>>) {
        *self.active.write().unwrap_or_else(|e| e.into_inner()) = identity;
    }

    pub fn prepare(&self, server: &ServerConfig) -> Result<Option<Arc<Identity>>> {
        server.validate_tls().map_err(anyhow::Error::msg)?;
        if server.tls.mode == TlsMode::Disabled {
            return Ok(None);
        }
        validate_origin(server)?;
        self.ensure_directory()?;
        let id = match server.tls.mode {
            TlsMode::SelfSigned => server.tls.self_signed_certificate.as_deref(),
            TlsMode::Custom => server.tls.custom_certificate.as_deref(),
            TlsMode::Disabled => unreachable!(),
        };
        let path = self.identity_path(id)?;
        if id.is_none() && !path.try_exists()? {
            let pem = generate_self_signed(server)?;
            self.write_new(&path, &pem)?;
        }
        let pem = read_identity(&path)?;
        Ok(Some(Arc::new(parse_identity(&pem, server)?)))
    }

    pub fn create_self_signed(&self, server: &ServerConfig) -> Result<String> {
        validate_origin(server)?;
        self.store(&generate_self_signed(server)?, server)
    }

    pub fn import(
        &self,
        certificate: &str,
        private_key: &str,
        server: &ServerConfig,
    ) -> Result<String> {
        ensure!(
            !certificate.is_empty() && !private_key.is_empty(),
            "证书和私钥必须一起提供"
        );
        ensure!(
            certificate.len() <= 128 * 1024 && private_key.len() <= 64 * 1024,
            "证书或私钥文件过大"
        );
        ensure!(
            server.public_origin.is_some(),
            "导入证书前请填写 HTTPS 访问地址"
        );
        ensure!(!certificate.contains("PRIVATE KEY"), "证书文件不能包含私钥");
        let pem = format!("{}\n{}\n", private_key.trim(), certificate.trim());
        self.store(pem.as_bytes(), server)
    }

    fn store(&self, pem: &[u8], server: &ServerConfig) -> Result<String> {
        // Validate before creating any file; an invalid upload leaves both modes intact.
        parse_identity(pem, server)?;
        self.ensure_directory()?;
        let mut random = [0_u8; 16];
        getrandom::fill(&mut random).map_err(|_| anyhow::anyhow!("无法生成证书标识"))?;
        let id = random
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>();
        self.write_new(&self.identity_path(Some(&id))?, pem)?;
        Ok(id)
    }

    /// Only called for an identity created by the current failed request.
    pub fn discard_unpublished(&self, id: &str) {
        if let Ok(path) = self.identity_path(Some(id)) {
            let _ = fs::remove_file(path);
        }
    }

    fn identity_path(&self, id: Option<&str>) -> Result<PathBuf> {
        match id {
            None => Ok(self.directory.join("self-signed.pem")),
            Some(id) => {
                ensure!(
                    id.len() == 32 && id.bytes().all(|b| b.is_ascii_hexdigit()),
                    "无效的证书标识"
                );
                Ok(self.directory.join(format!("{id}.pem")))
            }
        }
    }

    fn ensure_directory(&self) -> Result<()> {
        match fs::create_dir(self.directory.as_ref()) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(e) => return Err(e).context("无法创建证书目录"),
        }
        let metadata = fs::symlink_metadata(self.directory.as_ref())?;
        ensure!(
            metadata.is_dir() && !metadata.file_type().is_symlink(),
            "证书目录不能是符号链接"
        );
        fs::set_permissions(self.directory.as_ref(), fs::Permissions::from_mode(0o700))?;
        Ok(())
    }

    fn write_new(&self, path: &Path, pem: &[u8]) -> Result<()> {
        let mut random = [0_u8; 8];
        getrandom::fill(&mut random).map_err(|_| anyhow::anyhow!("无法生成临时文件名"))?;
        let temporary = self
            .directory
            .join(format!(".pending-{:016x}", u64::from_ne_bytes(random)));
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&temporary)
            .context("无法保存证书")?;
        let result = (|| -> Result<()> {
            file.write_all(pem)?;
            file.sync_all()?;
            // Publish only a fully written identity; hard_link cannot overwrite
            // another process's file or an existing certificate.
            fs::hard_link(&temporary, path)?;
            fs::File::open(self.directory.as_ref())?.sync_all()?;
            Ok(())
        })();
        drop(file);
        let _ = fs::remove_file(&temporary);
        result.context("无法完整保存证书")
    }
}

fn validate_origin(server: &ServerConfig) -> Result<()> {
    if let Some(origin) = &server.public_origin {
        let origin = NormalizedOrigin::parse(origin).map_err(anyhow::Error::msg)?;
        ensure!(
            !origin.is_http(),
            "启用 HTTPS 时访问地址必须以 https:// 开头"
        );
    }
    Ok(())
}

fn read_identity(path: &Path) -> Result<Vec<u8>> {
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)
        .context("无法读取证书，请重新导入或检查证书文件权限")?;
    let metadata = file.metadata()?;
    ensure!(
        metadata.is_file() && metadata.permissions().mode() & 0o077 == 0,
        "证书私钥文件必须是权限 0600 的普通文件"
    );
    let mut pem = Vec::new();
    file.take(MAX_PEM_BYTES as u64 + 1).read_to_end(&mut pem)?;
    ensure!(pem.len() <= MAX_PEM_BYTES, "证书文件过大");
    Ok(pem)
}

fn certificate_names(cert: &X509) -> Vec<String> {
    cert.subject_alt_names()
        .into_iter()
        .flatten()
        .filter_map(|name| {
            if let Some(dns) = name.dnsname() {
                return Some(dns.to_owned());
            }
            let bytes = name.ipaddress()?;
            match bytes.len() {
                4 => Some(std::net::Ipv4Addr::from(<[u8; 4]>::try_from(bytes).ok()?).to_string()),
                16 => Some(std::net::Ipv6Addr::from(<[u8; 16]>::try_from(bytes).ok()?).to_string()),
                _ => None,
            }
        })
        .collect()
}

fn covers_host(names: &[String], host: &str) -> bool {
    if let Ok(ip) = host.parse::<IpAddr>() {
        return names
            .iter()
            .any(|name| name.parse::<IpAddr>().ok() == Some(ip));
    }
    names.iter().any(|name| {
        name.eq_ignore_ascii_case(host)
            || name.strip_prefix("*.").is_some_and(|suffix| {
                host.split_once('.').is_some_and(|(label, rest)| {
                    !label.is_empty() && rest.eq_ignore_ascii_case(suffix)
                })
            })
    })
}

fn parse_identity(pem: &[u8], server: &ServerConfig) -> Result<Identity> {
    ensure!(pem.len() <= MAX_PEM_BYTES, "证书文件过大");
    validate_origin(server)?;
    let certificates = X509::stack_from_pem(pem).context("无法解析 PEM 证书链")?;
    ensure!(
        !certificates.is_empty() && certificates.len() <= 8,
        "证书链必须包含 1–8 张证书"
    );
    let private_key =
        PKey::private_key_from_pem(pem).context("无法解析私钥，请使用未加密的 PEM 私钥")?;
    ensure!(
        private_key.security_bits() >= 112,
        "私钥强度不足，请使用 RSA 2048 位或更强的密钥"
    );
    let leaf = &certificates[0];
    ensure!(
        leaf.public_key()?.public_eq(&private_key),
        "证书与私钥不匹配"
    );
    let now = Asn1Time::days_from_now(0)?;
    for cert in &certificates {
        ensure!(
            cert.not_before().compare(&now)? != Ordering::Greater,
            "证书尚未生效，请检查证书和设备时间"
        );
        ensure!(
            cert.not_after().compare(&now)? == Ordering::Greater,
            "证书已过期，请导入新证书"
        );
    }
    let names = certificate_names(leaf);
    if let Some(origin) = &server.public_origin {
        let origin = NormalizedOrigin::parse(origin).map_err(anyhow::Error::msg)?;
        ensure!(
            covers_host(&names, origin.hostname()),
            "证书未包含访问地址中的域名或 IP"
        );
    }
    // Check the order/signatures of the supplied chain. Client trust still depends
    // on the issuing CA; accepting an upload never asserts public trust.
    for pair in certificates.windows(2) {
        ensure!(
            pair[0].issuer_name().to_der()? == pair[1].subject_name().to_der()?,
            "证书链顺序错误"
        );
        let issuer_key = pair[1].public_key()?;
        ensure!(pair[0].verify(&issuer_key)?, "证书链签名校验失败");
    }
    let mut certificate_pem = Vec::new();
    let chain = certificates
        .iter()
        .map(|cert| {
            certificate_pem.extend(cert.to_pem()?);
            Ok(CertificateDer::from(cert.to_der()?))
        })
        .collect::<Result<Vec<_>>>()?;
    let key = PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(
        private_key.private_key_to_pkcs8()?,
    ));
    let mut config = rustls::ServerConfig::builder_with_provider(Arc::new(
        rustls::crypto::ring::default_provider(),
    ))
    .with_safe_default_protocol_versions()?
    .with_no_client_auth()
    .with_single_cert(chain, key)?;
    config.alpn_protocols = vec![b"http/1.1".to_vec()];
    let common_name = |name: &openssl::x509::X509NameRef| {
        name.entries_by_nid(Nid::COMMONNAME)
            .next()
            .map(|e| String::from_utf8_lossy(e.data().as_slice()).into_owned())
            .unwrap_or_default()
    };
    let fingerprint = Sha256::digest(leaf.to_der()?)
        .iter()
        .map(|b| format!("{b:02X}"))
        .collect::<Vec<_>>()
        .join(":");
    Ok(Identity {
        config: Arc::new(config),
        certificate: CertificateStatus {
            subject: common_name(leaf.subject_name()),
            issuer: common_name(leaf.issuer_name()),
            names,
            not_after: leaf.not_after().to_string(),
            expires_in_days: now.diff(leaf.not_after())?.days,
            fingerprint_sha256: fingerprint,
        },
        certificate_pem: String::from_utf8(certificate_pem)?,
    })
}

fn generate_self_signed(server: &ServerConfig) -> Result<Vec<u8>> {
    let group = EcGroup::from_curve_name(Nid::X9_62_PRIME256V1)?;
    let key = PKey::from_ec_key(EcKey::generate(&group)?)?;
    let mut name = X509NameBuilder::new()?;
    name.append_entry_by_nid(Nid::COMMONNAME, "WingmanKVM")?;
    let name = name.build();
    let mut certificate = X509::builder()?;
    certificate.set_version(2)?;
    let mut serial = BigNum::new()?;
    serial.rand(128, MsbOption::MAYBE_ZERO, false)?;
    let serial = serial.to_asn1_integer()?;
    certificate.set_serial_number(&serial)?;
    certificate.set_subject_name(&name)?;
    certificate.set_issuer_name(&name)?;
    certificate.set_pubkey(&key)?;
    let not_before = Asn1Time::days_from_now(0)?;
    let not_after = Asn1Time::days_from_now(3650)?;
    certificate.set_not_before(&not_before)?;
    certificate.set_not_after(&not_after)?;
    certificate.append_extension(BasicConstraints::new().critical().build()?)?;
    certificate.append_extension(KeyUsage::new().critical().digital_signature().build()?)?;
    certificate.append_extension(ExtendedKeyUsage::new().server_auth().build()?)?;
    let mut names = SubjectAlternativeName::new();
    names
        .dns("localhost")
        .dns("wingmankvm.local")
        .ip("127.0.0.1")
        .ip("::1");
    if let Some(origin) = &server.public_origin {
        let origin = NormalizedOrigin::parse(origin).map_err(anyhow::Error::msg)?;
        if origin.hostname().parse::<IpAddr>().is_ok() {
            names.ip(origin.hostname());
        } else {
            names.dns(origin.hostname());
        }
    }
    if let Ok(ip) = server.listen_address.parse::<IpAddr>() {
        if !ip.is_unspecified() && !ip.is_loopback() {
            names.ip(&ip.to_string());
        }
    }
    certificate.append_extension(names.build(&certificate.x509v3_context(None, None))?)?;
    certificate.sign(&key, MessageDigest::sha256())?;
    let mut pem = key.private_key_to_pem_pkcs8()?;
    pem.extend(certificate.build().to_pem()?);
    Ok(pem)
}
