use reqwest::Url;
use std::net::{IpAddr, SocketAddr};

pub fn public_addresses(addresses: &[SocketAddr]) -> bool {
    !addresses.is_empty() && addresses.iter().all(|address| public_ip(address.ip()))
}

pub fn public_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => {
            let [a, b, c, _] = ip.octets();
            !ip.is_private()
                && !ip.is_loopback()
                && !ip.is_link_local()
                && !ip.is_broadcast()
                && !ip.is_documentation()
                && a != 0
                && a < 224
                && !(a == 100 && (64..=127).contains(&b))
                && !(a == 192 && b == 0 && c == 0)
                && !(a == 198 && (b == 18 || b == 19))
        }
        IpAddr::V6(ip) => {
            if let Some(ip) = ip.to_ipv4_mapped() {
                return public_ip(IpAddr::V4(ip));
            }
            let s = ip.segments();
            // Only global unicast; exclude special-purpose 2001 ranges and 6to4.
            s[0] & 0xe000 == 0x2000
                && !(s[0] == 0x2001 && (s[1] < 0x200 || s[1] == 0xdb8))
                && s[0] != 0x2002
                && !(s[0] == 0x3fff && s[1] < 0x1000)
        }
    }
}

pub fn safe_url(value: &str) -> Option<Url> {
    if value.len() > 8192 {
        return None;
    }
    let mut url = Url::parse(value).ok()?;
    if !matches!(url.scheme(), "http" | "https")
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return None;
    }
    let host = url.host_str()?.trim_matches(['[', ']']);
    match host.parse::<IpAddr>() {
        Err(_) => {
            let host = host.trim_end_matches('.').to_ascii_lowercase();
            if host == "localhost" || host.ends_with(".localhost") || !host.contains('.') {
                return None;
            }
        }
        Ok(ip) if !public_ip(ip) => return None,
        _ => {}
    }
    url.set_fragment(None);
    Some(url)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn urls_and_dns_addresses_must_be_public() {
        for value in [
            "https://example.com",
            "http://example.com/test",
            "https://[2606:4700::1111]",
        ] {
            assert!(safe_url(value).is_some(), "{value}");
        }
        for value in [
            "file:///etc/passwd",
            "ftp://example.com",
            "data:text/html,x",
            "javascript:alert(1)",
            "tauri://localhost",
            "http://localhost",
            "http://localhost.",
            "http://127.0.0.1",
            "http://127.1",
            "http://192.168.1.1",
            "http://10.0.0.1",
            "http://172.31.0.1",
            "http://169.254.169.254",
            "http://0.0.0.0",
            "http://[::1]",
            "http://[fc00::1]",
            "http://[fe80::1]",
            "http://[::ffff:127.0.0.1]",
            "https://user:pass@example.com",
        ] {
            assert!(safe_url(value).is_none(), "{value}");
        }
        for ip in [
            "127.0.0.1",
            "10.1.1.1",
            "100.64.0.1",
            "224.0.0.1",
            "::1",
            "fe80::1",
            "2002:7f00:1::",
        ] {
            assert!(!public_ip(ip.parse().unwrap()), "{ip}");
        }
    }

    #[test]
    fn mixed_dns_answers_and_private_redirects_are_rejected() {
        let public = "93.184.216.34:443".parse().unwrap();
        let private = "127.0.0.1:443".parse().unwrap();
        assert!(public_addresses(&[public]));
        assert!(!public_addresses(&[]));
        assert!(!public_addresses(&[public, private]));
        let base = safe_url("https://example.com/start").unwrap();
        assert!(safe_url(base.join("/next").unwrap().as_str()).is_some());
        for location in [
            "http://10.0.0.1/private",
            "//localhost/private",
            "http://[::1]/private",
            "file:///etc/passwd",
        ] {
            assert!(safe_url(base.join(location).unwrap().as_str()).is_none());
        }
    }
}
