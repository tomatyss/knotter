use crate::source::VcfSource;
use crate::{Result, SyncError};

#[cfg(feature = "dav-sync")]
mod imp {
    use super::{Result, SyncError, VcfSource};
    use quick_xml::events::Event;
    use quick_xml::Reader;
    use reqwest::blocking::Client;
    use reqwest::Method;
    use std::time::Duration;
    use url::Url;

    const REPORT_BODY: &str = r#"<?xml version="1.0" encoding="utf-8" ?>
<card:addressbook-query xmlns:d="DAV:" xmlns:card="urn:ietf:params:xml:ns:carddav">
  <d:prop>
    <d:getetag/>
    <card:address-data/>
  </d:prop>
</card:addressbook-query>
"#;

    #[derive(Debug, Clone)]
    pub struct CardDavSource {
        addressbook_url: String,
        username: String,
        password: String,
        user_agent: Option<String>,
    }

    impl CardDavSource {
        pub fn new(
            addressbook_url: String,
            username: String,
            password: String,
            user_agent: Option<String>,
        ) -> Self {
            Self {
                addressbook_url,
                username,
                password,
                user_agent,
            }
        }
    }

    impl VcfSource for CardDavSource {
        fn source_name(&self) -> &'static str {
            "carddav"
        }

        fn fetch_vcf(&self) -> Result<String> {
            fetch_vcards(
                &self.addressbook_url,
                &self.username,
                &self.password,
                self.user_agent.as_deref(),
            )
        }
    }

    pub fn fetch_vcards(
        addressbook_url: &str,
        username: &str,
        password: &str,
        user_agent: Option<&str>,
    ) -> Result<String> {
        let url = Url::parse(addressbook_url)?;
        if url.scheme() != "https" {
            return Err(SyncError::Parse("carddav url must use https".to_string()));
        }
        let client = Client::builder()
            .user_agent(user_agent.unwrap_or("knotter"))
            .timeout(Duration::from_secs(30))
            .connect_timeout(Duration::from_secs(10))
            .build()?;
        let report_method = Method::from_bytes(b"REPORT")
            .map_err(|_| SyncError::Parse("invalid REPORT method".to_string()))?;

        let response = client
            .request(report_method, url)
            .basic_auth(username, Some(password))
            .header("Depth", "1")
            .header("Content-Type", "application/xml; charset=utf-8")
            .header("Accept", "application/xml")
            .body(REPORT_BODY)
            .send()?
            .error_for_status()?;

        let body = response.text()?;
        let cards = parse_address_data(&body)?;
        Ok(join_vcards(cards))
    }

    fn join_vcards(cards: Vec<String>) -> String {
        let mut out = String::new();
        for card in cards {
            let trimmed = card.trim_end();
            if trimmed.is_empty() {
                continue;
            }
            out.push_str(trimmed);
            out.push('\n');
        }
        out
    }

    fn parse_address_data(body: &str) -> Result<Vec<String>> {
        let mut reader = Reader::from_str(body);
        reader.trim_text(false);

        let mut buf = Vec::new();
        let mut cards = Vec::new();
        let mut current = String::new();
        let mut in_address_data = false;

        loop {
            match reader.read_event_into(&mut buf) {
                Ok(Event::Start(ref event)) if is_address_data(event.local_name().as_ref()) => {
                    in_address_data = true;
                    current.clear();
                }
                Ok(Event::End(ref event)) if is_address_data(event.local_name().as_ref()) => {
                    in_address_data = false;
                    if !current.trim().is_empty() {
                        let normalized = normalize_vcard_indentation(&current);
                        if !normalized.trim().is_empty() {
                            cards.push(normalized);
                        }
                    }
                    current.clear();
                }
                Ok(Event::Text(event)) if in_address_data => {
                    let text = event
                        .unescape()
                        .map_err(|err| SyncError::Parse(err.to_string()))?;
                    current.push_str(&text);
                }
                Ok(Event::CData(event)) if in_address_data => {
                    let text = String::from_utf8_lossy(event.as_ref());
                    current.push_str(&text);
                }
                Ok(Event::Eof) => break,
                Ok(_) => {}
                Err(err) => return Err(SyncError::Parse(err.to_string())),
            }
            buf.clear();
        }

        Ok(cards)
    }

    fn is_address_data(name: &[u8]) -> bool {
        name.eq_ignore_ascii_case(b"address-data")
    }

    fn normalize_vcard_indentation(raw: &str) -> String {
        let normalized = normalize_line_endings(raw);
        let mut lines: Vec<&str> = normalized.lines().collect();
        while matches!(lines.first(), Some(line) if line.trim().is_empty()) {
            lines.remove(0);
        }
        while matches!(lines.last(), Some(line) if line.trim().is_empty()) {
            lines.pop();
        }
        if lines.is_empty() {
            return String::new();
        }

        let common_indent = common_indent_for_vcard_lines(&lines);
        if common_indent == 0 {
            return lines.join("\n");
        }

        let mut out = String::new();
        for (idx, line) in lines.iter().enumerate() {
            let mut chars = line.chars();
            let mut remaining = common_indent;
            while remaining > 0 {
                match chars.next() {
                    Some(' ') | Some('\t') => remaining -= 1,
                    Some(other) => {
                        out.push(other);
                        break;
                    }
                    None => break,
                }
            }
            for ch in chars {
                out.push(ch);
            }
            if idx + 1 < lines.len() {
                out.push('\n');
            }
        }

        out
    }

    fn common_indent_for_vcard_lines(lines: &[&str]) -> usize {
        let mut min_indent: Option<usize> = None;
        for line in lines {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            let upper = trimmed.to_ascii_uppercase();
            if upper.starts_with("BEGIN:VCARD") || upper.starts_with("END:VCARD") {
                continue;
            }
            let indent = line
                .chars()
                .take_while(|ch| *ch == ' ' || *ch == '\t')
                .count();
            min_indent = Some(match min_indent {
                Some(current) => current.min(indent),
                None => indent,
            });
            if min_indent == Some(0) {
                break;
            }
        }

        min_indent.unwrap_or(0)
    }

    fn normalize_line_endings(input: &str) -> std::borrow::Cow<'_, str> {
        if !input.contains('\r') {
            return std::borrow::Cow::Borrowed(input);
        }

        let mut out = String::with_capacity(input.len());
        let mut chars = input.chars().peekable();
        while let Some(ch) = chars.next() {
            if ch == '\r' {
                if matches!(chars.peek(), Some('\n')) {
                    chars.next();
                }
                out.push('\n');
            } else {
                out.push(ch);
            }
        }
        std::borrow::Cow::Owned(out)
    }

    #[cfg(test)]
    mod tests {
        use super::parse_address_data;
        use crate::vcf::parse_vcf;

        #[test]
        fn parses_address_data_entries() {
            let xml = r#"
<d:multistatus xmlns:d="DAV:" xmlns:card="urn:ietf:params:xml:ns:carddav">
  <d:response>
    <d:propstat>
      <d:prop>
        <card:address-data>BEGIN:VCARD
FN:Ada Lovelace
END:VCARD
        </card:address-data>
      </d:prop>
    </d:propstat>
  </d:response>
  <d:response>
    <d:propstat>
      <d:prop>
        <card:address-data>BEGIN:VCARD
FN:Grace Hopper
END:VCARD
        </card:address-data>
      </d:prop>
    </d:propstat>
  </d:response>
</d:multistatus>
"#;
            let cards = parse_address_data(xml).expect("parse");
            assert_eq!(cards.len(), 2);
            assert!(cards[0].contains("Ada Lovelace"));
            assert!(cards[1].contains("Grace Hopper"));
        }

        #[test]
        fn parses_indented_address_data_without_breaking_vcard() {
            let xml = r#"
<d:multistatus xmlns:d="DAV:" xmlns:card="urn:ietf:params:xml:ns:carddav">
  <d:response>
    <d:propstat>
      <d:prop>
        <card:address-data>
          BEGIN:VCARD
          VERSION:3.0
          FN:Ada Lovelace
          NOTE:Line one
           continued
          END:VCARD
        </card:address-data>
      </d:prop>
    </d:propstat>
  </d:response>
</d:multistatus>
"#;
            let cards = parse_address_data(xml).expect("parse");
            assert_eq!(cards.len(), 1);
            let parsed = parse_vcf(&cards[0]).expect("vcf parse");
            assert_eq!(parsed.contacts.len(), 1);
            assert_eq!(parsed.contacts[0].display_name, "Ada Lovelace");
        }

        #[test]
        fn normalizes_indented_lines_when_begin_is_unindented() {
            let xml = r#"
<d:multistatus xmlns:d="DAV:" xmlns:card="urn:ietf:params:xml:ns:carddav">
  <d:response>
    <d:propstat>
      <d:prop>
        <card:address-data>BEGIN:VCARD
          VERSION:3.0
          FN:Ada Lovelace
          NOTE:Line one
           continued
          END:VCARD
        </card:address-data>
      </d:prop>
    </d:propstat>
  </d:response>
</d:multistatus>
"#;
            let cards = parse_address_data(xml).expect("parse");
            assert_eq!(cards.len(), 1);
            let parsed = parse_vcf(&cards[0]).expect("vcf parse");
            assert_eq!(parsed.contacts.len(), 1);
            assert_eq!(parsed.contacts[0].display_name, "Ada Lovelace");
        }
    }
}

#[cfg(not(feature = "dav-sync"))]
mod imp {
    use super::{Result, SyncError, VcfSource};

    #[derive(Debug, Clone)]
    pub struct CardDavSource {
        addressbook_url: String,
        username: String,
        password: String,
        user_agent: Option<String>,
    }

    impl CardDavSource {
        pub fn new(
            addressbook_url: String,
            username: String,
            password: String,
            user_agent: Option<String>,
        ) -> Self {
            Self {
                addressbook_url,
                username,
                password,
                user_agent,
            }
        }
    }

    impl VcfSource for CardDavSource {
        fn source_name(&self) -> &'static str {
            "carddav"
        }

        fn fetch_vcf(&self) -> Result<String> {
            let _ = (
                &self.addressbook_url,
                &self.username,
                &self.password,
                &self.user_agent,
            );
            Err(SyncError::Unavailable(
                "CardDAV import requires the dav-sync feature".to_string(),
            ))
        }
    }

    pub fn fetch_vcards(
        _addressbook_url: &str,
        _username: &str,
        _password: &str,
        _user_agent: Option<&str>,
    ) -> Result<String> {
        Err(SyncError::Unavailable(
            "CardDAV import requires the dav-sync feature".to_string(),
        ))
    }
}

pub use imp::{fetch_vcards, CardDavSource};
