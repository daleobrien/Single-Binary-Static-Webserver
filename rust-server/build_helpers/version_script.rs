use crate::build_helpers::processing;
use crate::build_helpers::utils;

pub(super) fn build_version_script(build_version: &str) -> (String, String) {
    let version_script_src = format!(
        r#"(function(){{const v="{build_version}";async function c(){{try{{const r=await fetch("/v",{{cache:"no-store",headers:{{"If-None-Match":v}}}});if(r.status===304)return;if(r.ok){{fetch(window.location.href,{{cache:"reload"}}).then(()=>location.reload())}}}}catch(_){{}}}}const[n]=performance.getEntriesByType("navigation");if(n&&n.transferSize===0)c();setInterval(c,60000)}})()"#
    );
    let version_js_minified = processing::minify_js_bytes(&version_script_src);
    let version_script_tag = format!(
        "<script>{}</script>",
        String::from_utf8_lossy(&version_js_minified)
    );
    let csp_script_hash = utils::sha256_base64(&version_js_minified);
    (version_script_tag, csp_script_hash)
}
