use crate::fingerprint::{V3WebFingerPrint, WebFingerPrintLib};
use futures::future::join_all;
use std::collections::{HashMap, HashSet};
use std::fmt;
use std::fmt::Write as FmtWrite;
use std::sync::Arc;
use url::Url;

#[derive(Debug)]
pub struct RawData {
    pub url: Url,
    pub path: String,
    pub headers: reqwest::header::HeaderMap,
    pub status_code: reqwest::StatusCode,
    pub text: String,
    pub favicon: HashMap<String, String>,
    pub next_url: Option<Url>,
}

impl fmt::Display for RawData {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let mut s = String::new();
        if let Ok(u) = self.url.join(&self.path) {
            let _ = write!(s, "Url: {}\r\n", u);
        }
        s.push_str("Headers:\r\n");
        s.push_str(&header_to_string(&self.headers));
        let _ = write!(s, "StatusCode: {}\r\n", self.status_code.as_u16());
        s.push_str("Text:\r\n");
        s.push_str(&self.text);
        s.push_str("\r\n");
        if !self.favicon.is_empty() {
            let _ = write!(s, "Favicon: {:#?}\r\n", self.favicon);
        }
        if let Some(next_url) = &self.next_url {
            let _ = write!(s, "NextUrl: {}\r\n", next_url);
        }
        write!(f, "{}", s)
    }
}

pub async fn check(
    raw_data: &Arc<RawData>,
    fingerprint_lib: &WebFingerPrintLib,
    debug: bool,
) -> HashMap<String, u32> {
    if debug {
        println!("{}", raw_data);
    }
    let mut futures_e = vec![];
    let mut web_name_set: HashMap<String, u32> = HashMap::new();
    for fingerprint in fingerprint_lib.special.iter() {
        futures_e.push(what_web(raw_data.clone(), fingerprint, debug));
    }
    for fingerprint in fingerprint_lib.index.iter() {
        futures_e.push(what_web(raw_data.clone(), fingerprint, debug));
    }
    if !raw_data.favicon.is_empty() {
        for fingerprint in fingerprint_lib.favicon.iter() {
            futures_e.push(what_web(raw_data.clone(), fingerprint, debug));
        }
    }
    let results = join_all(futures_e).await;
    for res in results {
        let (is_match, match_web_fingerprint) = res;
        if is_match {
            web_name_set.insert(
                match_web_fingerprint.name.clone(),
                match_web_fingerprint.priority,
            );
        }
    }
    web_name_set
}

pub async fn what_web(
    raw_data: Arc<RawData>,
    fingerprint: &V3WebFingerPrint,
    debug: bool,
) -> (bool, &V3WebFingerPrint) {
    // 默认匹配不到
    let mut default_result = (false, fingerprint);
    // 匹配FaviconHash
    if !fingerprint.match_rules.favicon_hash.is_empty() {
        let mut hash_set = HashSet::new();
        for (_key, value) in raw_data.favicon.iter() {
            hash_set.insert(value);
        }
        // 请求中没有找到FaviconHash
        if hash_set.is_empty() {
            return default_result;
        }
        let mut fph_set = HashSet::new();
        for fph in fingerprint.match_rules.favicon_hash.iter() {
            fph_set.insert(fph);
        }
        if hash_set.intersection(&fph_set).count() == 0 {
            return default_result;
        }
    }
    // 关键词匹配
    let not_match_status_code = || {
        fingerprint.match_rules.status_code != 0
            && raw_data.status_code.as_u16() != fingerprint.match_rules.status_code
    };
    if not_match_status_code() {
        return default_result;
    }
    for (k, v) in &fingerprint.match_rules.headers {
        let matcher_part = header_to_string(&raw_data.headers);
        if k == "set-cookie" && !matcher_part.contains(v) {
            return default_result;
        }
        if raw_data.headers.contains_key(k) {
            let is_match = matcher_part.to_lowercase().find(&v.to_lowercase());
            if is_match == None && v != "*" {
                return default_result;
            }
        } else {
            return default_result;
        }
    }
    for keyword in &fingerprint.match_rules.keyword {
        if raw_data.text.find(&keyword.to_lowercase()) == None {
            return default_result;
        }
    }
    default_result.0 = true;
    if debug {
        println!("Matching fingerprint{:#?}", fingerprint);
    }
    default_result
}

fn header_to_string(headers: &reqwest::header::HeaderMap) -> String {
    let mut header_string = String::new();
    for (k, v) in headers.clone() {
        if let Some(k) = k {
            header_string.push_str(k.as_str());
            header_string.push_str(": ");
        }
        header_string.push_str(v.to_str().unwrap_or_default());
        header_string.push_str("\r\n");
    }
    header_string
}
