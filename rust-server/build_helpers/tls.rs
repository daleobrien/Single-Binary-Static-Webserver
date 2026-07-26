use std::fs;
use std::net::{IpAddr, Ipv4Addr};
use std::path::Path;

use rcgen::{date_time_ymd, CertificateParams, DistinguishedName, DnType, KeyPair, SanType};

/// Set up TLS certificates.
///
/// If `certs/cert.pem` and `certs/key.pem` exist, they are converted to DER and written
/// to the output directory. Otherwise a self-signed certificate is generated using the
/// `HOSTNAME` env var (defaults to `localhost`).
pub fn setup_tls(out_dir: &str) {
    let certs_dir = "../certs";
    let cert_der_path = format!("{out_dir}/cert.der");
    let key_der_path = format!("{out_dir}/key.der");

    let cert_pem_path = format!("{certs_dir}/cert.pem");
    let key_pem_path = format!("{certs_dir}/key.pem");

    if Path::new(&cert_pem_path).exists() && Path::new(&key_pem_path).exists() {
        eprintln!("build.rs: using existing TLS certs from {certs_dir}/");
        let cert_pem = fs::read(&cert_pem_path).expect("failed to read cert.pem");
        let key_pem = fs::read(&key_pem_path).expect("failed to read key.pem");

        let cert_der: Vec<u8> = rustls_pemfile::certs(&mut cert_pem.as_slice())
            .next()
            .expect("no certificate found in cert.pem")
            .expect("invalid certificate in cert.pem")
            .as_ref()
            .to_vec();
        let key_der: Vec<u8> = rustls_pemfile::private_key(&mut key_pem.as_slice())
            .expect("failed to parse key.pem")
            .expect("no private key found in key.pem")
            .secret_der()
            .to_vec();

        fs::write(&cert_der_path, &cert_der).expect("failed to write cert.der");
        fs::write(&key_der_path, &key_der).expect("failed to write key.der");
    } else {
        let hostname = std::env::var("HOSTNAME").unwrap_or_else(|_| "localhost".into());
        eprintln!("build.rs: generating self-signed TLS certificate for {hostname}");

        let mut params = CertificateParams::new(vec![hostname.clone()])
            .expect("failed to create certificate params");
        let mut dn = DistinguishedName::new();
        dn.push(DnType::CommonName, &hostname);
        params.distinguished_name = dn;
        params.subject_alt_names = vec![
            SanType::DnsName(hostname.as_str().try_into().unwrap()),
            SanType::IpAddress(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1))),
        ];
        params.not_before = date_time_ymd(2025, 1, 1);
        params.not_after = date_time_ymd(2035, 1, 1);

        let key_pair = KeyPair::generate().expect("failed to generate key pair");
        let cert = params
            .self_signed(&key_pair)
            .expect("failed to self-sign certificate");

        fs::write(&cert_der_path, cert.der()).expect("failed to write cert.der");
        fs::write(&key_der_path, key_pair.serialize_der()).expect("failed to write key.der");
    }

    println!("cargo:rerun-if-changed=../certs/");
}
