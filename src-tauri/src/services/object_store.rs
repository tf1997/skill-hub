use anyhow::{anyhow, Context, Result};
use chrono::Utc;
use reqwest::{header, Url};
use sha2::{Digest, Sha256};

use crate::{
    admin_config,
    db::{COMPILED_SOURCE_BUCKET, COMPILED_SOURCE_ENDPOINT, COMPILED_SOURCE_REGION},
    models::Source,
};

fn extract_xml_values(input: &str, tag: &str) -> Vec<String> {
    let start = format!("<{tag}>");
    let end = format!("</{tag}>");
    let mut rest = input;
    let mut values = Vec::new();
    while let Some(start_index) = rest.find(&start) {
        let after_start = &rest[start_index + start.len()..];
        let Some(end_index) = after_start.find(&end) else {
            break;
        };
        values.push(xml_unescape(&after_start[..end_index]));
        rest = &after_start[end_index + end.len()..];
    }
    values
}

fn xml_unescape(value: &str) -> String {
    value
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&amp;", "&")
}

pub(crate) fn object_url(source: &Source, object_path: &str) -> Result<Url> {
    let base = format!(
        "{}/{}/{}",
        source.endpoint.trim_end_matches('/'),
        source.bucket.trim_matches('/'),
        object_path.trim_start_matches('/')
    );
    Url::parse(&base).context("对象 URL 无效")
}

pub(crate) struct AdminObjectClient {
    source: Source,
    client: reqwest::Client,
}

impl AdminObjectClient {
    pub(crate) fn new() -> Self {
        Self {
            source: Source {
                id: "admin-publisher".to_string(),
                name: "Admin Publisher".to_string(),
                endpoint: COMPILED_SOURCE_ENDPOINT.to_string(),
                bucket: COMPILED_SOURCE_BUCKET.to_string(),
                region: COMPILED_SOURCE_REGION.map(ToString::to_string),
                enabled: true,
                last_sync_at: None,
            },
            client: reqwest::Client::new(),
        }
    }

    pub(crate) async fn get_text(&self, object_path: &str) -> Result<String> {
        let url = object_url(&self.source, object_path)?;
        let signed = signed_request_headers("GET", &url, self.source.region.as_deref(), b"")?;
        let mut request = self.client.get(url);
        for (name, value) in signed {
            request = request.header(name, value);
        }
        request
            .send()
            .await
            .with_context(|| format!("读取 MinIO 对象失败: {object_path}"))?
            .error_for_status()
            .with_context(|| format!("MinIO 对象响应失败: {object_path}"))?
            .text()
            .await
            .with_context(|| format!("读取 MinIO 对象内容失败: {object_path}"))
    }

    pub(crate) async fn get_optional_text(&self, object_path: &str) -> Result<Option<String>> {
        let url = object_url(&self.source, object_path)?;
        let signed = signed_request_headers("GET", &url, self.source.region.as_deref(), b"")?;
        let mut request = self.client.get(url);
        for (name, value) in signed {
            request = request.header(name, value);
        }
        let response = request
            .send()
            .await
            .with_context(|| format!("读取 MinIO 对象失败: {object_path}"))?;
        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }
        Ok(Some(
            response
                .error_for_status()
                .with_context(|| format!("MinIO 对象响应失败: {object_path}"))?
                .text()
                .await
                .with_context(|| format!("读取 MinIO 对象内容失败: {object_path}"))?,
        ))
    }

    pub(crate) async fn get_bytes(&self, object_path: &str) -> Result<Vec<u8>> {
        let url = object_url(&self.source, object_path)?;
        let signed = signed_request_headers("GET", &url, self.source.region.as_deref(), b"")?;
        let mut request = self.client.get(url);
        for (name, value) in signed {
            request = request.header(name, value);
        }
        Ok(request
            .send()
            .await
            .with_context(|| format!("读取 MinIO 对象失败: {object_path}"))?
            .error_for_status()
            .with_context(|| format!("MinIO 对象响应失败: {object_path}"))?
            .bytes()
            .await
            .with_context(|| format!("读取 MinIO 对象内容失败: {object_path}"))?
            .to_vec())
    }

    pub(crate) async fn get_optional_json<T>(&self, object_path: &str) -> Result<Option<T>>
    where
        T: for<'de> serde::Deserialize<'de>,
    {
        let Some(text) = self.get_optional_text(object_path).await? else {
            return Ok(None);
        };
        serde_json::from_str(&text)
            .with_context(|| format!("解析 JSON 对象失败: {object_path}"))
            .map(Some)
    }

    pub(crate) async fn put_json<T: serde::Serialize>(
        &self,
        object_path: &str,
        value: &T,
    ) -> Result<()> {
        let bytes = serde_json::to_vec_pretty(value)?;
        self.put_bytes(object_path, bytes, "application/json; charset=utf-8")
            .await
    }

    pub(crate) async fn put_text(
        &self,
        object_path: &str,
        value: &str,
        content_type: &str,
    ) -> Result<()> {
        self.put_bytes(object_path, value.as_bytes().to_vec(), content_type)
            .await
    }

    pub(crate) async fn put_bytes(
        &self,
        object_path: &str,
        bytes: Vec<u8>,
        content_type: &str,
    ) -> Result<()> {
        let url = object_url(&self.source, object_path)?;
        let signed = signed_request_headers("PUT", &url, self.source.region.as_deref(), &bytes)?;
        let mut request = self
            .client
            .put(url)
            .header(header::CONTENT_TYPE, content_type)
            .body(bytes);
        for (name, value) in signed {
            request = request.header(name, value);
        }
        request
            .send()
            .await
            .with_context(|| format!("写入 MinIO 对象失败: {object_path}"))?
            .error_for_status()
            .with_context(|| format!("MinIO 写入响应失败: {object_path}"))?;
        Ok(())
    }

    pub(crate) async fn list_objects(&self, prefix: &str) -> Result<Vec<String>> {
        let mut results = Vec::new();
        let mut continuation: Option<String> = None;

        loop {
            let mut url = Url::parse(&format!(
                "{}/{}",
                self.source.endpoint.trim_end_matches('/'),
                self.source.bucket.trim_matches('/')
            ))
            .context("对象列表 URL 无效")?;
            {
                let mut pairs = url.query_pairs_mut();
                pairs.append_pair("list-type", "2");
                pairs.append_pair("prefix", prefix);
                pairs.append_pair("max-keys", "1000");
                if let Some(token) = continuation.as_deref() {
                    pairs.append_pair("continuation-token", token);
                }
            }

            let signed = signed_request_headers("GET", &url, self.source.region.as_deref(), b"")?;
            let mut request = self.client.get(url);
            for (name, value) in signed {
                request = request.header(name, value);
            }
            let text = request
                .send()
                .await
                .with_context(|| format!("列出 MinIO 前缀失败: {prefix}"))?
                .error_for_status()
                .with_context(|| format!("MinIO 前缀列表响应失败: {prefix}"))?
                .text()
                .await
                .context("读取 MinIO 前缀列表失败")?;

            results.extend(extract_xml_values(&text, "Key"));
            continuation = extract_xml_values(&text, "NextContinuationToken")
                .into_iter()
                .next();
            if continuation.is_none() {
                break;
            }
        }

        Ok(results)
    }
}

pub(crate) async fn fetch_admin_mac_allowlist() -> Result<admin_config::MacAllowlist> {
    let client = AdminObjectClient::new();
    let object_path = admin_config::allowlist_path();
    let text = client.get_text(object_path).await?;

    admin_config::parse_mac_allowlist(&text).with_context(|| {
        format!(
            "解析 MinIO MAC 白名单失败，请检查 {} 的 JSON 格式",
            object_path
        )
    })
}

pub(crate) fn signed_request_headers(
    method: &str,
    url: &Url,
    region: Option<&str>,
    payload: &[u8],
) -> Result<Vec<(&'static str, String)>> {
    let request_time = Utc::now();
    let amz_date = request_time.format("%Y%m%dT%H%M%SZ").to_string();
    let short_date = request_time.format("%Y%m%d").to_string();
    let region = region
        .filter(|value| !value.trim().is_empty())
        .unwrap_or("us-east-1");
    let host = url_host(url)?;
    let payload_hash = sha256_hex(payload);

    let canonical_request = format!(
        "{}\n{}\n{}\nhost:{}\nx-amz-content-sha256:{}\nx-amz-date:{}\n\nhost;x-amz-content-sha256;x-amz-date\n{}",
        method,
        canonical_uri(url),
        canonical_query(url),
        host,
        payload_hash,
        amz_date,
        payload_hash
    );
    let credential_scope = format!("{}/{}/s3/aws4_request", short_date, region);
    let string_to_sign = format!(
        "AWS4-HMAC-SHA256\n{}\n{}\n{}",
        amz_date,
        credential_scope,
        sha256_hex(canonical_request.as_bytes())
    );
    let signing_key = sigv4_signing_key(admin_config::publisher_secret_key(), &short_date, region);
    let signature = hex_lower(&hmac_sha256(&signing_key, string_to_sign.as_bytes()));
    let authorization = format!(
        "AWS4-HMAC-SHA256 Credential={}/{}, SignedHeaders=host;x-amz-content-sha256;x-amz-date, Signature={}",
        admin_config::publisher_access_key(),
        credential_scope,
        signature
    );

    Ok(vec![
        ("host", host),
        ("x-amz-content-sha256", payload_hash),
        ("x-amz-date", amz_date),
        ("authorization", authorization),
    ])
}

pub(crate) fn url_host(url: &Url) -> Result<String> {
    let host = url
        .host_str()
        .ok_or_else(|| anyhow!("MinIO endpoint 缺少 host"))?;
    Ok(match url.port() {
        Some(port) => format!("{host}:{port}"),
        None => host.to_string(),
    })
}

pub(crate) fn canonical_uri(url: &Url) -> String {
    let path = url.path();
    if path.is_empty() {
        "/".to_string()
    } else {
        path.to_string()
    }
}

pub(crate) fn canonical_query(url: &Url) -> String {
    let mut pairs = url
        .query_pairs()
        .map(|(key, value)| (uri_encode(&key, true), uri_encode(&value, true)))
        .collect::<Vec<_>>();
    pairs.sort();
    pairs
        .into_iter()
        .map(|(key, value)| format!("{key}={value}"))
        .collect::<Vec<_>>()
        .join("&")
}

pub(crate) fn uri_encode(value: &str, encode_slash: bool) -> String {
    let mut out = String::new();
    for byte in value.as_bytes() {
        let ch = *byte as char;
        let keep = ch.is_ascii_alphanumeric()
            || matches!(ch, '-' | '_' | '.' | '~')
            || (!encode_slash && ch == '/');
        if keep {
            out.push(ch);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

pub(crate) fn sigv4_signing_key(secret: &str, short_date: &str, region: &str) -> Vec<u8> {
    let date_key = hmac_sha256(format!("AWS4{secret}").as_bytes(), short_date.as_bytes());
    let date_region_key = hmac_sha256(&date_key, region.as_bytes());
    let date_region_service_key = hmac_sha256(&date_region_key, b"s3");
    hmac_sha256(&date_region_service_key, b"aws4_request")
}

pub(crate) fn hmac_sha256(key: &[u8], message: &[u8]) -> Vec<u8> {
    const BLOCK_SIZE: usize = 64;

    let mut normalized_key = if key.len() > BLOCK_SIZE {
        Sha256::digest(key).to_vec()
    } else {
        key.to_vec()
    };
    normalized_key.resize(BLOCK_SIZE, 0);

    let mut outer_key_pad = [0x5c_u8; BLOCK_SIZE];
    let mut inner_key_pad = [0x36_u8; BLOCK_SIZE];
    for index in 0..BLOCK_SIZE {
        outer_key_pad[index] ^= normalized_key[index];
        inner_key_pad[index] ^= normalized_key[index];
    }

    let mut inner = Sha256::new();
    inner.update(inner_key_pad);
    inner.update(message);
    let inner_hash = inner.finalize();

    let mut outer = Sha256::new();
    outer.update(outer_key_pad);
    outer.update(inner_hash);
    outer.finalize().to_vec()
}

pub(crate) fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex_lower(&hasher.finalize())
}

pub(crate) fn hex_lower(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(HEX[(byte >> 4) as usize] as char);
        output.push(HEX[(byte & 0x0f) as usize] as char);
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_s3_list_keys() {
        let xml = r#"<ListBucketResult>
          <Contents><Key>draft/gitlab/skills/a/SKILL.md</Key></Contents>
          <Contents><Key>draft/gitlab/skills/a/README.md</Key></Contents>
          <NextContinuationToken>abc&amp;123</NextContinuationToken>
        </ListBucketResult>"#;

        assert_eq!(
            extract_xml_values(xml, "Key"),
            vec![
                "draft/gitlab/skills/a/SKILL.md".to_string(),
                "draft/gitlab/skills/a/README.md".to_string()
            ]
        );
        assert_eq!(
            extract_xml_values(xml, "NextContinuationToken"),
            vec!["abc&123".to_string()]
        );
    }
}
