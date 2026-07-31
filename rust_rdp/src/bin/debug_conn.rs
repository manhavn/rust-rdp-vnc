use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::{DigitallySignedStruct, SignatureScheme};
use std::sync::Arc;
use tokio_rustls::TlsConnector;

use ironrdp_async::{connect_begin, connect_finalize, mark_as_upgraded};
use ironrdp_connector::{
    BitmapConfig, ClientConnector, Config, Credentials, DesktopSize, ServerName as RdpServerName,
};
use ironrdp_dvc::DrdynvcClient;
use ironrdp_egfx::client::{BitmapUpdate, GraphicsPipelineClient, GraphicsPipelineHandler};
use ironrdp_tokio::TokioFramed;

struct DummyGfxHandler;
impl GraphicsPipelineHandler for DummyGfxHandler {
    fn on_bitmap_updated(&mut self, _update: &BitmapUpdate) {
        println!("DummyGfxHandler: on_bitmap_updated");
    }
}

struct SimpleLogger;
impl log::Log for SimpleLogger {
    fn enabled(&self, metadata: &log::Metadata) -> bool {
        metadata.level() <= log::Level::Info
            || (metadata.level() <= log::Level::Debug
                && (metadata
                    .target()
                    .starts_with("ironrdp_connector::connection")
                    || metadata
                        .target()
                        .starts_with("ironrdp_connector::connection_activation")
                    || metadata.target().starts_with("ironrdp_async::framed")))
    }
    fn log(&self, record: &log::Record) {
        if self.enabled(record.metadata()) {
            println!(
                "[{}] [{}]: {}",
                record.level(),
                record.target(),
                record.args()
            );
        }
    }
    fn flush(&self) {}
}
static LOGGER: SimpleLogger = SimpleLogger;

#[derive(Debug)]
struct NoVerify;

impl ServerCertVerifier for NoVerify {
    fn verify_server_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        Ok(HandshakeSignatureValid::assertion())
    }

    fn verify_tls13_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        Ok(HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        vec![
            SignatureScheme::RSA_PKCS1_SHA1,
            SignatureScheme::ECDSA_SHA1_Legacy,
            SignatureScheme::RSA_PKCS1_SHA256,
            SignatureScheme::RSA_PKCS1_SHA384,
            SignatureScheme::RSA_PKCS1_SHA512,
            SignatureScheme::ECDSA_NISTP256_SHA256,
            SignatureScheme::ECDSA_NISTP384_SHA384,
            SignatureScheme::ECDSA_NISTP521_SHA512,
            SignatureScheme::ED25519,
            SignatureScheme::RSA_PSS_SHA256,
            SignatureScheme::RSA_PSS_SHA384,
            SignatureScheme::RSA_PSS_SHA512,
        ]
    }
}

struct SimpleNetworkClient;

impl ironrdp_async::NetworkClient for SimpleNetworkClient {
    async fn send(
        &mut self,
        _request: &ironrdp_connector::sspi::generator::NetworkRequest,
    ) -> ironrdp_connector::ConnectorResult<Vec<u8>> {
        Err(ironrdp_connector::general_err!(
            "SSPI network request not supported"
        ))
    }
}

fn read_der_length(der: &[u8], cursor: &mut usize) -> Option<usize> {
    if *cursor >= der.len() {
        return None;
    }
    let first = der[*cursor];
    *cursor += 1;

    if first < 0x80 {
        Some(first as usize)
    } else {
        let n_bytes = (first & 0x7F) as usize;
        if n_bytes == 0 || n_bytes > 4 || *cursor + n_bytes > der.len() {
            return None;
        }
        let mut len = 0;
        for _ in 0..n_bytes {
            len = (len << 8) | (der[*cursor] as usize);
            *cursor += 1;
        }
        Some(len)
    }
}

fn extract_raw_public_key(spki_der: &[u8]) -> Option<Vec<u8>> {
    let mut cursor = 0;

    // Read SEQUENCE tag
    if cursor >= spki_der.len() || spki_der[cursor] != 0x30 {
        return None;
    }
    cursor += 1;

    // Read SEQUENCE length
    let _seq_len = read_der_length(spki_der, &mut cursor)?;

    // Read algorithm (AlgorithmIdentifier SEQUENCE)
    if cursor >= spki_der.len() || spki_der[cursor] != 0x30 {
        return None;
    }
    cursor += 1;
    let alg_len = read_der_length(spki_der, &mut cursor)?;
    cursor += alg_len; // Skip AlgorithmIdentifier

    // Read subjectPublicKey BIT STRING (Tag 0x03)
    if cursor >= spki_der.len() || spki_der[cursor] != 0x03 {
        return None;
    }
    cursor += 1;

    let bit_str_len = read_der_length(spki_der, &mut cursor)?;
    if cursor + bit_str_len > spki_der.len() {
        return None;
    }

    // The BIT STRING value starts with a single byte indicating the number of unused bits (usually 0)
    let unused_bits = spki_der[cursor];
    if unused_bits != 0 {
        return None;
    }

    Some(spki_der[cursor + 1..cursor + bit_str_len].to_vec())
}

#[tokio::main]
async fn main() {
    let _ = log::set_logger(&LOGGER);
    // CredSSP trace output may include serialized credential payloads.
    log::set_max_level(log::LevelFilter::Debug);

    let Some(path) = std::env::args_os().nth(1) else {
        eprintln!("Usage: debug_conn <connection.rdp>");
        std::process::exit(2);
    };
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| {
        eprintln!(
            "Could not read {}: {e}",
            std::path::Path::new(&path).display()
        );
        std::process::exit(2);
    });
    let mut host_str = String::new();
    let mut port = 3389;
    let mut user_str = String::new();
    let mut pass_str = String::new();
    let mut domain_str = String::new();
    let mut width = 1280;
    let mut height = 720;
    for raw in text.lines() {
        let Some((key, rest)) = raw.trim().split_once(':') else {
            continue;
        };
        let value = rest
            .split_once(':')
            .map(|(_, value)| value)
            .unwrap_or(rest)
            .trim();
        match key.trim().to_ascii_lowercase().as_str() {
            "full address" => {
                if let Some((host, parsed_port)) = value.rsplit_once(':') {
                    if let Ok(parsed_port) = parsed_port.parse() {
                        host_str = host.to_string();
                        port = parsed_port;
                    } else {
                        host_str = value.to_string();
                    }
                } else {
                    host_str = value.to_string();
                }
            }
            "username" => user_str = value.to_string(),
            "password" => pass_str = value.to_string(),
            "domain" => domain_str = value.to_string(),
            "desktopwidth" => width = value.parse().unwrap_or(width),
            "desktopheight" => height = value.parse().unwrap_or(height),
            _ => {}
        }
    }
    if host_str.is_empty() {
        eprintln!("Connection file has no full address");
        std::process::exit(2);
    }

    println!(
        "Starting debug connection to {}:{} as {} (domain: {})...",
        host_str, port, user_str, domain_str
    );

    struct AttemptConfig {
        enable_credssp: bool,
        domain: Option<String>,
        desc: &'static str,
    }

    let mut attempts = Vec::new();
    if !domain_str.is_empty() {
        attempts.push(AttemptConfig {
            enable_credssp: true,
            domain: Some(domain_str.clone()),
            desc: "CredSSP (NLA) with domain",
        });
    }
    attempts.push(AttemptConfig {
        enable_credssp: true,
        domain: None,
        desc: "CredSSP (NLA) without domain",
    });
    attempts.push(AttemptConfig {
        enable_credssp: false,
        domain: None,
        desc: "TLS Security (no NLA)",
    });

    let addr = format!("{}:{}", host_str, port);
    let mut last_error = String::new();

    for (idx, attempt) in attempts.iter().enumerate() {
        println!(
            "\n=== TRYING ATTEMPT {}/{} ({}) ===",
            idx + 1,
            attempts.len(),
            attempt.desc
        );

        match tokio::net::TcpStream::connect(&addr).await {
            Ok(tcp_stream) => {
                let local_addr = match tcp_stream.local_addr() {
                    Ok(addr) => addr,
                    Err(e) => {
                        println!("Socket local_addr error: {}", e);
                        last_error = format!("Socket local_addr error: {}", e);
                        continue;
                    }
                };

                let credentials = Credentials::UsernamePassword {
                    username: user_str.clone(),
                    password: pass_str.clone(),
                };

                let config = Config {
                    desktop_size: DesktopSize {
                        width: width as u16,
                        height: height as u16,
                    },
                    desktop_scale_factor: 100,
                    enable_tls: true,
                    enable_credssp: attempt.enable_credssp,
                    credentials,
                    domain: attempt.domain.clone(),
                    client_build: 2600,
                    client_name: "RustRDPVNC".to_string(),
                    keyboard_type: ironrdp_pdu::gcc::KeyboardType::IbmEnhanced,
                    keyboard_subtype: 0,
                    keyboard_functional_keys_count: 12,
                    keyboard_layout: 1033,
                    ime_file_name: String::new(),
                    bitmap: Some(BitmapConfig {
                        lossy_compression: true,
                        color_depth: 32,
                        codecs: ironrdp_pdu::rdp::capability_sets::BitmapCodecs::default(),
                    }),
                    dig_product_id: String::new(),
                    client_dir: String::new(),
                    alternate_shell: String::new(),
                    work_dir: String::new(),
                    platform: ironrdp_pdu::rdp::capability_sets::MajorPlatformType::UNIX,
                    hardware_id: None,
                    request_data: None,
                    autologon: true,
                    enable_audio_playback: false,
                    performance_flags: ironrdp_pdu::rdp::client_info::PerformanceFlags::empty(),
                    license_cache: None,
                    timezone_info: ironrdp_pdu::rdp::client_info::TimezoneInfo::default(),
                    compression_type: None,
                    enable_server_pointer: false,
                    pointer_software_rendering: false,
                    multitransport_flags: None,
                };

                let mut connector = ClientConnector::new(config, local_addr);
                let mut drdynvc_client = DrdynvcClient::new();
                let gfx_client = GraphicsPipelineClient::new(Box::new(DummyGfxHandler), None);
                drdynvc_client.attach_dynamic_channel(gfx_client);
                connector.attach_static_channel(drdynvc_client);

                let mut framed = TokioFramed::new(tcp_stream);

                let should_upgrade = match connect_begin(&mut framed, &mut connector).await {
                    Ok(su) => su,
                    Err(e) => {
                        println!("connect_begin failed: {:?}", e);
                        last_error = format!("Handshake failed: {:?}", e);
                        continue;
                    }
                };

                println!("Upgrading connection to TLS...");
                let (tcp_stream, leftover) = framed.into_inner();

                let tls_config = rustls::ClientConfig::builder_with_provider(Arc::new(
                    rustls::crypto::ring::default_provider(),
                ))
                .with_safe_default_protocol_versions()
                .unwrap()
                .dangerous()
                .with_custom_certificate_verifier(Arc::new(NoVerify))
                .with_no_client_auth();

                let server_name = match ServerName::try_from(host_str.clone()) {
                    Ok(sn) => sn.to_owned(),
                    Err(e) => {
                        println!("Invalid host name: {:?}", e);
                        last_error = format!("Invalid host name: {:?}", e);
                        continue;
                    }
                };

                let tls_connector = TlsConnector::from(Arc::new(tls_config));
                let tls_stream = match tls_connector.connect(server_name, tcp_stream).await {
                    Ok(ts) => ts,
                    Err(e) => {
                        println!("TLS connect failed: {:?}", e);
                        last_error = format!("TLS Connection Failed: {:?}", e);
                        continue;
                    }
                };

                let (_, connection) = tls_stream.get_ref();
                let certs = connection.peer_certificates().unwrap_or(&[]);
                let server_public_key = if let Some(cert_der) = certs.first() {
                    match picky::x509::Cert::from_der(cert_der.as_ref()) {
                        Ok(cert) => match cert.public_key().to_der() {
                            Ok(spki_der) => match extract_raw_public_key(&spki_der) {
                                Some(raw_key) => {
                                    println!(
                                        "Successfully extracted raw public key (len: {})",
                                        raw_key.len()
                                    );
                                    raw_key
                                }
                                None => {
                                    println!("Failed to extract raw public key from SPKI DER, falling back to SPKI DER");
                                    spki_der
                                }
                            },
                            Err(e) => {
                                println!("picky Cert to_der error: {:?}", e);
                                Vec::new()
                            }
                        },
                        Err(e) => {
                            println!("picky Cert from_der error: {:?}", e);
                            Vec::new()
                        }
                    }
                } else {
                    Vec::new()
                };

                let mut framed = TokioFramed::new_with_leftover(tls_stream, leftover);
                let upgraded = mark_as_upgraded(should_upgrade, &mut connector);

                let mut network_client = SimpleNetworkClient;
                let rdp_server_name = RdpServerName::new(host_str.clone());

                match connect_finalize(
                    upgraded,
                    connector,
                    &mut framed,
                    &mut network_client,
                    rdp_server_name,
                    server_public_key,
                    None,
                )
                .await
                {
                    Ok(_res) => {
                        println!("SUCCESS! Connection finalized successfully!");
                        return;
                    }
                    Err(e) => {
                        println!("connect_finalize failed: {:?}", e);
                        last_error = format!("Finalize failed: {:?}", e);
                        continue;
                    }
                }
            }
            Err(e) => {
                println!("TCP Connect failed: {}", e);
                last_error = format!("Network connection refused: {}", e);
                continue;
            }
        }
    }

    println!("\nALL ATTEMPTS FAILED! Last error: {}", last_error);
}
