//! The release Dockerfile's bases are pinned by digest (#24; tacplus-dev/roadmap
//! decisions/security-gates-ship-only-2026-11-23.md, point 2): a floating tag lets a
//! rebuild of the same commit ship a different, unscanned base.

use std::path::Path;

/// Each `FROM` whose image is not `<image>[:<tag>]@sha256:<64 hex>`, as its line.
/// A `FROM` naming an earlier stage, or `scratch`, has no registry image to pin.
fn unpinned(dockerfile: &str) -> Vec<String> {
    let mut stages = vec!["scratch".to_string()];
    let mut found = Vec::new();
    for line in dockerfile.lines() {
        let mut words = line.split_whitespace();
        if !words.next().is_some_and(|w| w.eq_ignore_ascii_case("from")) {
            continue;
        }
        let mut words = words.skip_while(|w| w.starts_with("--"));
        let Some(image) = words.next() else {
            found.push(line.to_string());
            continue;
        };
        let pinned = image.split_once("@sha256:").is_some_and(|(_, digest)| {
            digest.len() == 64 && digest.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
        });
        if !pinned && !stages.contains(&image.to_ascii_lowercase()) {
            found.push(line.to_string());
        }
        if let (Some(as_), Some(name)) = (words.next(), words.next()) {
            if as_.eq_ignore_ascii_case("as") {
                stages.push(name.to_ascii_lowercase());
            }
        }
    }
    found
}

#[test]
fn every_from_is_pinned_by_digest() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("Dockerfile");
    let dockerfile = std::fs::read_to_string(&path).expect("Dockerfile");
    assert!(
        dockerfile.lines().any(|l| l.trim_start().to_ascii_lowercase().starts_with("from ")),
        "Dockerfile has no FROM"
    );
    assert_eq!(unpinned(&dockerfile), Vec::<String>::new(), "FROM without @sha256:<digest>");
}

#[test]
fn a_floating_tag_is_refused() {
    let digest = "dac90a25ffe03ebaace90cda886b081e61ab6cd77ac47eda51f7bcc15c320765";
    assert_eq!(unpinned("FROM gcr.io/distroless/cc-debian13\n"), ["FROM gcr.io/distroless/cc-debian13"]);
    assert_eq!(unpinned("from debian:trixie-slim AS build\n"), ["from debian:trixie-slim AS build"]);
    assert_eq!(unpinned("FROM --platform=$BUILDPLATFORM debian@sha256:abc\n").len(), 1);
    assert!(unpinned(&format!("FROM gcr.io/distroless/cc-debian13:latest@sha256:{digest}\n")).is_empty());
    assert!(unpinned(&format!(
        "FROM --platform=$BUILDPLATFORM debian:trixie@sha256:{digest} AS build\nFROM build\nFROM scratch\n"
    ))
    .is_empty());
}
