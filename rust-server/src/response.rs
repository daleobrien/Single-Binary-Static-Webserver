use bytes::Bytes;
use http_body_util::Full;

use crate::Asset;

/// Build a full response for an asset's body (h1/h2 path).
///
/// Headers are stored as `&[(&str, &str)]` and converted to `HeaderName`/`HeaderValue`
/// at request time via `from_static` (a const fn — validation already happened at
/// compile time, so the call is just a pointer wrap). This avoids the per-request
/// `HeaderMap::clone()` hash-table allocation entirely.
#[inline]
pub fn response_for_asset(asset: &Asset) -> hyper::Response<Full<Bytes>> {
    let status =
        hyper::StatusCode::from_u16(asset.status_code).expect("invalid status code at compile time");
    let mut resp = hyper::Response::new(Full::new(Bytes::from_static(asset.body)));
    *resp.status_mut() = status;
    let headers = resp.headers_mut();
    headers.reserve(asset.headers.len());
    for &(name, value) in asset.headers {
        headers.insert(
            hyper::header::HeaderName::from_static(name),
            hyper::header::HeaderValue::from_static(value),
        );
    }
    resp
}
