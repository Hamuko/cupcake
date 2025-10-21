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
