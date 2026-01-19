use std::borrow::Cow;

pub fn format_payload<'a>(rule_type: &str, payload: &'a str) -> Cow<'a, str> {
    let rule_type = rule_type.to_uppercase();
    match rule_type.as_str() {
        "AND" | "OR" | "NOT" => {
            let items = parse_logic_payload(payload);
            let formatted = items
                .into_iter()
                .map(|(t, p)| format_single_rule(&t, &p))
                .collect::<Vec<_>>()
                .join(", ");
            Cow::Owned(formatted)
        }
        "SUB-RULE" => {
            let (t, p) = parse_inner_payload(payload);
            Cow::Owned(format_single_rule(&t, &p))
        }
        _ => Cow::Borrowed(payload),
    }
}

fn format_single_rule(t: &str, p: &str) -> String {
    let formatted_p = format_payload(t, p);
    if formatted_p.is_empty() { t.to_string() } else { format!("{}: {}", t, formatted_p) }
}

fn parse_inner_payload(item: &str) -> (String, String) {
    let item = item.trim();
    // Strip outer parens: (TYPE,PAYLOAD) -> TYPE,PAYLOAD
    let content =
        if item.starts_with('(') && item.ends_with(')') { &item[1..item.len() - 1] } else { item };

    if let Some((t, p)) = content.split_once(',') {
        (t.trim().to_string(), p.trim().to_string())
    } else {
        (content.trim().to_string(), String::new())
    }
}

fn parse_logic_payload(payload: &str) -> Vec<(String, String)> {
    let mut results = Vec::new();
    let content = payload.trim();
    if content.len() < 2 {
        return results;
    }

    let inner = if content.starts_with('(') && content.ends_with(')') {
        &content[1..content.len() - 1]
    } else {
        content
    };

    let mut start_byte = 0;
    let mut depth = 0;

    for (byte_idx, c) in inner.char_indices() {
        match c {
            '(' => depth += 1,
            ')' => depth -= 1,
            ',' if depth == 0 => {
                if byte_idx > start_byte {
                    results.push(parse_inner_payload(&inner[start_byte..byte_idx]));
                }
                start_byte = byte_idx + 1; // ',' is 1 byte
            }
            _ => {}
        }
    }
    if start_byte < inner.len() {
        results.push(parse_inner_payload(&inner[start_byte..]));
    }

    results
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_payload() {
        assert_eq!(format_payload("DOMAIN", "google.com"), "google.com");

        // AND,((DOMAIN,baidu.com),(NETWORK,UDP))
        let payload = "((DOMAIN,baidu.com),(NETWORK,UDP))";
        assert_eq!(format_payload("AND", payload), "DOMAIN: baidu.com, NETWORK: UDP");

        // NOT,((DOMAIN,baidu.com))
        let payload = "((DOMAIN,baidu.com))";
        assert_eq!(format_payload("NOT", payload), "DOMAIN: baidu.com");

        // Nested logic
        let payload = "((AND,((A,1),(B,2))),(C,3))";
        assert_eq!(format_payload("OR", payload), "AND: A: 1, B: 2, C: 3");

        // SUB-RULE
        assert_eq!(format_payload("SUB-RULE", "(NETWORK,tcp)"), "NETWORK: tcp");
    }

    #[test]
    fn test_unicode_payload() {
        match std::panic::catch_unwind(|| {
            let payload = "((PROCESS-NAME,测试),(PROCESS-NAME-REGEX,test★))";
            format_payload("AND", payload)
        }) {
            Ok(res) => assert_eq!(res, "PROCESS-NAME: 测试, PROCESS-NAME-REGEX: test★"),
            Err(_) => panic!("Panic occurred during unicode parsing"),
        }
    }
}
