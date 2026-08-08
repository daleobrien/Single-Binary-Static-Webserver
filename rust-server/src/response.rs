use bytes::Bytes;
use http_body_util::Full;

use crate::Asset;
use crate::{EMBED_BROTLI, EMBED_GZIP, EMBED_ZSTD};

/// Supported content encodings for runtime `Accept-Encoding` negotiation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContentEncoding {
    /// No compression — serve the uncompressed body.
    Identity,
    /// Gzip compression.
    Gzip,
    /// Brotli compression.
    Brotli,
    /// Zstandard compression.
    Zstd,
}

/// Parse the `Accept-Encoding` request header and select the best encoding
/// supported by both the client and server.
///
/// Returns [`ContentEncoding::Identity`] if no acceptable encoding is found
/// or if the header is missing/malformed.
///
/// Called from the binary request-handler path; the `#[allow(dead_code)]`
/// suppresses the lib-only false positive from `cargo check --lib`.
#[allow(dead_code)]
pub(crate) fn parse_accept_encoding(header_value: Option<&hyper::header::HeaderValue>) -> ContentEncoding {
    let value = match header_value.and_then(|v| v.to_str().ok()) {
        Some(v) => v,
        None => return ContentEncoding::Identity,
    };

    // Track the highest quality value seen for each encoding we support.
    // Default quality is 1.0; identity defaults to a very low value so it
    // only wins when nothing else is acceptable.
    let mut best_q: f32 = 0.0001; // identity baseline
    let mut best_encoding = ContentEncoding::Identity;
    // Higher preference wins when q-values are equal.
    let mut best_pref: u8 = 0;

    for token in value.split(',') {
        let token = token.trim();
        if token.is_empty() {
            continue;
        }

        // Split encoding name from parameters (e.g., "br;q=0.9" → "br", "q=0.9")
        let (name, params) = match token.split_once(';') {
            Some((n, rest)) => (n.trim(), Some(rest.trim())),
            None => (token, None),
        };

        let q = params
            .and_then(|p| {
                p.split(';')
                    .find(|part| part.trim().starts_with("q="))
                    .and_then(|qpart| qpart.trim().strip_prefix("q="))
                    .and_then(|qval| qval.parse::<f32>().ok())
            })
            .unwrap_or(1.0);

        // Map the encoding name, with a preference score for tie-breaking:
        // brotli(3) > zstd(2) > gzip(1) > identity(0).
        // Disabled encodings (EMBED_*=false) are filtered out here.
        let candidate: Option<(ContentEncoding, f32, u8)> = match name {
            "br" if EMBED_BROTLI => Some((ContentEncoding::Brotli, q, 3)),
            "zstd" if EMBED_ZSTD => Some((ContentEncoding::Zstd, q, 2)),
            "gzip" | "x-gzip" if EMBED_GZIP => Some((ContentEncoding::Gzip, q, 1)),
            "identity" => Some((ContentEncoding::Identity, q, 0)),
            "*" => {
                // Wildcard matches all — use the best we have at q.
                let (best_enc, best_pref_candidate) = if EMBED_BROTLI {
                    (ContentEncoding::Brotli, 3)
                } else if EMBED_ZSTD {
                    (ContentEncoding::Zstd, 2)
                } else if EMBED_GZIP {
                    (ContentEncoding::Gzip, 1)
                } else {
                    (ContentEncoding::Identity, 0)
                };
                if q > best_q || (q == best_q && best_pref_candidate > best_pref) {
                    best_encoding = best_enc;
                    best_q = q;
                    best_pref = best_pref_candidate;
                }
                continue;
            }
            _ => None, // unsupported encoding (deflate, compress, etc.)
        };

        if let Some((enc, q_val, pref)) = candidate {
            if q_val > best_q || (q_val == best_q && pref > best_pref) {
                best_q = q_val;
                best_encoding = enc;
                best_pref = pref;
            }
        }
    }

    best_encoding
}

/// Return the body slice for the given encoding.
#[inline]
pub fn body_for_encoding(asset: &Asset, encoding: ContentEncoding) -> &'static [u8] {
    match encoding {
        ContentEncoding::Identity => asset.body,
        ContentEncoding::Gzip => asset.body_gzip,
        ContentEncoding::Brotli => asset.body_brotli,
        ContentEncoding::Zstd => asset.body_zstd,
    }
}

/// Return the body length for the given encoding.
///
/// Called from the binary request-handler path; the `#[allow(dead_code)]`
/// suppresses the lib-only false positive from `cargo check --lib`.
#[allow(dead_code)]
#[inline]
pub(crate) fn content_length_for_encoding(asset: &Asset, encoding: ContentEncoding) -> u64 {
    match encoding {
        ContentEncoding::Identity => asset.uncompressed_length as u64,
        ContentEncoding::Gzip => asset.gzip_length as u64,
        ContentEncoding::Brotli => asset.brotli_length as u64,
        ContentEncoding::Zstd => asset.zstd_length as u64,
    }
}

/// Return the pre-baked header slice for the given encoding.
///
/// Each variant includes all static headers plus per-encoding
/// `Content-Length` and `Content-Encoding` (where applicable),
/// computed at compile time so the request path has zero branches
/// and zero allocations for header insertion.
#[inline]
pub fn headers_for_encoding(asset: &Asset, encoding: ContentEncoding) -> &'static [(&'static str, &'static str)] {
    match encoding {
        ContentEncoding::Identity => asset.headers_identity,
        ContentEncoding::Gzip => asset.headers_gzip,
        ContentEncoding::Brotli => asset.headers_brotli,
        ContentEncoding::Zstd => asset.headers_zstd,
    }
}

/// Build a full response for an asset's body (h1/h2 path).
///
/// The `encoding` parameter selects which variant to serve based on the
/// client's `Accept-Encoding` header.
///
/// Headers are stored as `&[(&str, &str)]` and converted to `HeaderName`/`HeaderValue`
/// at request time via `from_static` (a const fn — validation already happened at
/// compile time, so the call is just a pointer wrap). This avoids the per-request
/// `HeaderMap::clone()` hash-table allocation entirely.
#[inline]
pub fn response_for_asset(
    asset: &Asset,
    encoding: ContentEncoding,
) -> hyper::Response<Full<Bytes>> {
    let body = body_for_encoding(asset, encoding);
    let status =
        hyper::StatusCode::from_u16(asset.status_code).expect("invalid status code at compile time");
    let mut resp = hyper::Response::new(Full::new(Bytes::from_static(body)));
    *resp.status_mut() = status;
    let hdrs = headers_for_encoding(asset, encoding);
    let headers = resp.headers_mut();
    headers.reserve(hdrs.len());
    for &(name, value) in hdrs {
        headers.insert(
            hyper::header::HeaderName::from_static(name),
            hyper::header::HeaderValue::from_static(value),
        );
    }
    resp
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── parse_accept_encoding ─────────────────────────────────────────────────

    #[test]
    fn parse_empty_returns_identity() {
        assert_eq!(parse_accept_encoding(None), ContentEncoding::Identity);
    }

    #[test]
    fn parse_gzip_returns_gzip() {
        let val = hyper::header::HeaderValue::from_static("gzip");
        assert_eq!(parse_accept_encoding(Some(&val)), ContentEncoding::Gzip);
    }

    #[test]
    fn parse_br_returns_brotli() {
        let val = hyper::header::HeaderValue::from_static("br");
        assert_eq!(parse_accept_encoding(Some(&val)), ContentEncoding::Brotli);
    }

    #[test]
    fn parse_zstd_returns_zstd() {
        let val = hyper::header::HeaderValue::from_static("zstd");
        assert_eq!(parse_accept_encoding(Some(&val)), ContentEncoding::Zstd);
    }

    #[test]
    fn parse_identity_returns_identity() {
        let val = hyper::header::HeaderValue::from_static("identity");
        assert_eq!(parse_accept_encoding(Some(&val)), ContentEncoding::Identity);
    }

    #[test]
    fn parse_multiple_picks_highest_quality() {
        let val = hyper::header::HeaderValue::from_static("gzip;q=0.5, br;q=1.0");
        assert_eq!(parse_accept_encoding(Some(&val)), ContentEncoding::Brotli);
    }

    #[test]
    fn parse_quality_zero_ignored() {
        let val = hyper::header::HeaderValue::from_static("br;q=0");
        assert_eq!(parse_accept_encoding(Some(&val)), ContentEncoding::Identity);
    }

    #[test]
    fn parse_wildcard_prefers_brotli() {
        let val = hyper::header::HeaderValue::from_static("*");
        assert_eq!(parse_accept_encoding(Some(&val)), ContentEncoding::Brotli);
    }

    #[test]
    fn parse_browser_like_accepts_br() {
        // Typical Chrome Accept-Encoding
        let val = hyper::header::HeaderValue::from_static("gzip, deflate, br");
        assert_eq!(parse_accept_encoding(Some(&val)), ContentEncoding::Brotli);
    }

    #[test]
    fn parse_browser_like_no_br_accepts_gzip() {
        // Older browser without brotli support
        let val = hyper::header::HeaderValue::from_static("gzip, deflate");
        assert_eq!(parse_accept_encoding(Some(&val)), ContentEncoding::Gzip);
    }

    #[test]
    fn parse_zstd_preferred_over_gzip() {
        let val = hyper::header::HeaderValue::from_static("gzip, zstd");
        assert_eq!(parse_accept_encoding(Some(&val)), ContentEncoding::Zstd);
    }

    // ── body_for_encoding / content_length_for_encoding ───────────────────────
    // These are tested implicitly through the integration tests and benchmarks.
}
