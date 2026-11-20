pub fn escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;").replace('\'', "&apos;")
}

pub fn unescape(s: &str) -> String {
    s.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&amp;", "&")
}

pub fn tag_text(xml: &str, tag: &str) -> Option<String> {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    let start = xml.find(&open)? + open.len();
    let end = xml[start..].find(&close)? + start;
    Some(unescape(xml[start..end].trim()))
}

pub fn attr(fragment: &str, name: &str) -> Option<String> {
    let pat = format!("{name}=\"");
    let mut from = 0;
    while let Some(i) = fragment[from..].find(&pat) {
        let at = from + i;
        if at == 0 || fragment.as_bytes()[at - 1] == b' ' {
            let start = at + pat.len();
            let end = fragment[start..].find('"')? + start;
            return Some(unescape(&fragment[start..end]));
        }
        from = at + pat.len();
    }
    None
}

pub fn open_tags<'a>(xml: &'a str, tag: &str) -> Vec<&'a str> {
    let pat = format!("<{tag} ");
    let mut out = vec![];
    let mut from = 0;
    while let Some(i) = xml[from..].find(&pat) {
        let start = from + i;
        let end = xml[start..].find('>').map(|e| start + e).unwrap_or(xml.len());
        out.push(&xml[start..end]);
        from = end;
    }
    out
}

pub fn service_control_url(xml: &str, kind: &str) -> Option<String> {
    service_field(xml, kind, "controlURL")
}

pub fn service_event_url(xml: &str, kind: &str) -> Option<String> {
    service_field(xml, kind, "eventSubURL")
}

fn service_field(xml: &str, kind: &str, field: &str) -> Option<String> {
    let mut from = 0;
    while let Some(i) = xml[from..].find("<service>") {
        let start = from + i;
        let end = xml[start..].find("</service>").map(|e| start + e)?;
        let block = &xml[start..end];
        if tag_text(block, "serviceType").map_or(false, |t| t.contains(kind)) {
            return tag_text(block, field).filter(|s| !s.is_empty());
        }
        from = end;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_av_transport() {
        let x = "<serviceList><service><serviceType>urn:schemas-upnp-org:service:RenderingControl:1</serviceType><controlURL>/rc</controlURL></service><service><serviceType>urn:schemas-upnp-org:service:AVTransport:1</serviceType><controlURL>/MediaRenderer/AVTransport/Control</controlURL></service></serviceList>";
        assert_eq!(service_control_url(x, "AVTransport").as_deref(), Some("/MediaRenderer/AVTransport/Control"));
    }

    #[test]
    fn attr_requires_word_boundary() {
        let f = "<ZoneGroupMember UUID=\"RINCON_1\" XZoneName=\"no\" ZoneName=\"Salon\"";
        assert_eq!(attr(f, "ZoneName").as_deref(), Some("Salon"));
        assert_eq!(attr(f, "UUID").as_deref(), Some("RINCON_1"));
    }

    #[test]
    fn escape_roundtrip() {
        assert_eq!(unescape(&escape("<a b=\"1\">&</a>")), "<a b=\"1\">&</a>");
    }
}
