use regex::Regex;
use std::sync::LazyLock;

/// Determine if the error message indicates a throttling error.
pub fn check_throttling_error(string: &str) -> Option<u64> {
    static RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"Guest logins are restricted to one per IP address per (\d+) seconds").unwrap()
    });
    let captures = RE.captures(string)?;
    let duration_capture = captures.get(1)?;
    duration_capture.as_str().parse::<u64>().ok()
}

/// Parse host from plain domain name or URL.
pub fn parse_domain(s: &str) -> Result<url::Host, String> {
    if let Ok(host) = url::Host::parse(s) {
        return Ok(host);
    };
    if let Ok(domain) = url::Url::parse(s)
        && let Some(host) = domain.host()
    {
        return Ok(host.to_owned());
    };
    Err(String::from("Not a valid domain or URL"))
}

#[cfg(test)]
mod tests {
    use test_case::test_case;

    #[test_case("Guest logins are restricted to one per IP address per 60 seconds.", Some(60); "normal error")]
    #[test_case("Guest logins are restricted to one per IP address per 9999 seconds.", Some(9999); "longer error")]
    #[test_case("Guest logins are restricted to one per IP address per N seconds.", None; "non-numerical error")]
    #[test_case("Something bad happened", None; "unknown error")]
    fn check_throttling_error(input: &str, expected: Option<u64>) {
        assert_eq!(super::check_throttling_error(input), expected);
    }

    #[test_case("cytu.be", Some("cytu.be"); "plain domain")]
    #[test_case("https://cytu.be", Some("cytu.be"); "URL")]
    #[test_case("@t!", None; "invalid characters")]
    fn parse_domain(input: &str, expected: Option<&str>) {
        let expected = match expected {
            Some(s) => Ok(url::Host::Domain(s.to_string())),
            None => Err(String::from("Not a valid domain or URL")),
        };
        assert_eq!(super::parse_domain(input), expected);
    }
}
