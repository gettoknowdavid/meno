//! `publish-bible` — upload one Bible seed payload to the CDN.
//!
//! ```text
//! cargo run --bin publish-bible -- --code asv --payload ../mobile/assets/bible/asv.seed
//! ```
//!
//! # What it uploads, and where
//!
//! The file goes to `bible/<code>/<version>.json.gz` — version and code come from the
//! catalogue entry for `--code`, never from the command line, so a payload cannot be
//! published under a version the endpoint does not advertise. The object is written with
//! `Content-Type: application/gzip` and a one-year `immutable` `Cache-Control`, which is
//! what lets the CDN keep it without revalidating.
//!
//! # What it refuses
//!
//! A file whose byte length or SHA-256 is not the one in
//! [`catalogue::TRANSLATIONS`](meno_api::modules::bible::catalogue::TRANSLATIONS). The
//! endpoint hands both values to clients as the thing to verify a download against, so a
//! publish that skipped the check would put a payload in the bucket that every device
//! rejects *after* downloading it. The error prints the entry's values next to the file's,
//! and nothing is uploaded.
//!
//! # Why it does not load the application's `Config`
//!
//! `Config` requires a database URL, a Redis URL and two JWT secrets, all of which the API
//! needs and none of which publishing a file does. This reads the six `STORAGE_*`
//! variables (after `dotenvy`, so a repository `.env` works as it does for the server),
//! says which one is missing by name, and builds the same `S3ObjectStore` the API builds
//! from the same settings.

// The workspace denies the print macros so request-serving code cannot log around
// `tracing`. This binary's only job is to write to the terminal, so it opts out — the one
// file in the crate that may.
#![allow(clippy::print_stdout, clippy::print_stderr)]

use std::process::ExitCode;
use std::sync::Arc;

use bytes::Bytes;
use meno_api::config::{Secret, StorageSettings};
use meno_api::infrastructure::storage::{ObjectStore, store_from_settings};
use meno_api::modules::bible::catalogue;
use meno_api::modules::bible::publish::publish;

fn main() -> ExitCode {
    // A blocking `main` with one small runtime: the whole tool is one upload, and the exit
    // code is the part a CI job reads.
    let runtime = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(error) => {
            eprintln!("publish-bible: could not start a runtime: {error}");
            return ExitCode::FAILURE;
        }
    };

    runtime.block_on(run())
}

async fn run() -> ExitCode {
    let arguments: Vec<String> = std::env::args().skip(1).collect();

    if arguments
        .iter()
        .any(|argument| argument == "--help" || argument == "-h")
    {
        println!("{}", usage());
        return ExitCode::SUCCESS;
    }

    let Some(code) = flag(&arguments, "code") else {
        eprintln!("publish-bible: --code is required\n\n{}", usage());
        return ExitCode::from(2);
    };

    let Some(payload_path) = flag(&arguments, "payload") else {
        eprintln!("publish-bible: --payload is required\n\n{}", usage());
        return ExitCode::from(2);
    };

    let Some(entry) = catalogue::entry(&code) else {
        eprintln!(
            "publish-bible: `{code}` is not in the catalogue. Known codes: {}",
            catalogue::TRANSLATIONS
                .iter()
                .map(|entry| entry.code)
                .collect::<Vec<_>>()
                .join(", ")
        );
        return ExitCode::from(2);
    };

    let payload = match std::fs::read(&payload_path) {
        Ok(bytes) => Bytes::from(bytes),
        Err(error) => {
            eprintln!("publish-bible: could not read {payload_path}: {error}");
            return ExitCode::FAILURE;
        }
    };

    let store = match storage() {
        Ok(store) => store,
        Err(message) => {
            eprintln!("publish-bible: {message}");
            return ExitCode::FAILURE;
        }
    };

    match publish(store.as_ref(), entry, payload).await {
        Ok(published) => {
            println!(
                "published {} v{} ({}) — {} bytes, sha256 {}",
                entry.code, entry.version, entry.name, published.bytes, published.sha256
            );
            // `storage` requires a public URL, so this is set in practice; printed from
            // the store rather than rebuilt here so the key is spelled in one place.
            match published.url {
                Some(url) => println!("{url}"),
                None => println!("no public URL: STORAGE_PUBLIC_URL is not set"),
            }
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("publish-bible: {error}");
            ExitCode::FAILURE
        }
    }
}

/// The value of `--name value`, if it was given one.
///
/// Hand-rolled for the same reason the converter hand-rolls its flags: one tool, two flags,
/// and a dependency that exists to parse them would be the largest thing in the file.
fn flag(arguments: &[String], name: &str) -> Option<String> {
    let position = arguments
        .iter()
        .position(|argument| argument == &format!("--{name}"))?;
    let value = arguments.get(position + 1)?;
    if value.starts_with("--") {
        return None;
    }
    Some(value.clone())
}

/// The store the six `STORAGE_*` variables describe.
///
/// `dotenvy` first, so the tool picks up the same `.env` the server does; then the
/// variables, each named in the error when it is missing. `STORAGE_ENABLED` is not
/// consulted — this tool exists to publish, and an operator running it has already decided
/// storage is on.
fn storage() -> Result<Arc<dyn ObjectStore>, String> {
    dotenvy::dotenv().ok();

    let settings = StorageSettings {
        endpoint: required("STORAGE_ENDPOINT")?,
        access_key: required("STORAGE_ACCESS_KEY")?,
        secret_key: Secret::new(required("STORAGE_SECRET_KEY")?),
        bucket: required("STORAGE_BUCKET")?,
        region: required("STORAGE_REGION")?,
        public_url: required("STORAGE_PUBLIC_URL")?,
    };

    store_from_settings(&settings)
        .map_err(|error| format!("the storage settings are unusable: {error}"))
}

/// One environment variable, or an error naming it.
fn required(name: &str) -> Result<String, String> {
    match std::env::var(name) {
        Ok(value) if !value.trim().is_empty() => Ok(value),
        _ => Err(format!("{name} is not set")),
    }
}

/// How to use the tool.
fn usage() -> String {
    format!(
        "publish-bible — upload one Bible seed payload to bible/<code>/<version>.json.gz\n\n\
         Usage:\n  \
         cargo run --bin publish-bible -- --code <code> --payload <path.json.gz>\n\n\
         Codes:  {}\n\
         Paths:  apps/mobile/assets/bible/<code>.seed (written by the converter's \
         --seed-output)\n\
         Env:    STORAGE_ENDPOINT, STORAGE_ACCESS_KEY, STORAGE_SECRET_KEY, STORAGE_BUCKET, \
         STORAGE_REGION, STORAGE_PUBLIC_URL",
        catalogue::TRANSLATIONS
            .iter()
            .map(|entry| format!("{} (v{}, {} bytes)", entry.code, entry.version, entry.bytes))
            .collect::<Vec<_>>()
            .join(", ")
    )
}
