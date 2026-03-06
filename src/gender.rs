use std::{
    ascii, fs, io,
    net::SocketAddr,
    path::{self, Path, PathBuf},
    str,
    sync::Arc,
};

use anyhow::{Context, Result, anyhow, bail};
use clap::Parser;
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer, pem::PemObject};

mod common;
fn main() { 
    let cert_path = Path::new("cert.der");
    let key_path = Path::new("key.der");
    println!("generating self-signed certificate");
    let cert = rcgen::generate_simple_self_signed(vec!["localhost".into()]).unwrap();
    let key = PrivatePkcs8KeyDer::from(cert.signing_key.serialize_der());
    let cert = cert.cert.der();
    fs::write(&cert_path, &cert)
        .context("failed to write certificate")
        .unwrap();
    fs::write(&key_path, key.secret_pkcs8_der())
        .context("failed to write private key")
        .unwrap();
}
