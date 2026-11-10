//! XenSearch-style query terms for VM and template lists.
use xen_core::xapi::VirtualMachine;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Field {
    Name,
    Uuid,
    Description,
    Os,
    Kind,
    Connection,
    PowerState,
    Host,
    Tag,
    Vcpus,
    Memory,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Comparison {
    Eq,
    Gt,
    Ge,
    Lt,
    Le,
}

impl Comparison {
    fn matches(self, actual: u64, expected: u64) -> bool {
        match self {
            Self::Eq => actual == expected,
            Self::Gt => actual > expected,
            Self::Ge => actual >= expected,
            Self::Lt => actual < expected,
            Self::Le => actual <= expected,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Term {
    negated: bool,
    field: Option<Field>,
    value: String,
    comparison: Option<Comparison>,
    number: Option<u64>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SearchQuery {
    terms: Vec<Term>,
}

impl SearchQuery {
    pub fn parse(input: &str) -> Self {
        let mut terms = Vec::new();
        for mut token in tokenize(input) {
            let negated = token.starts_with('-') && token.len() > 1;
            if negated {
                token.remove(0);
            }
            let (field, value) = match token.split_once(':') {
                Some((field, value)) => match field.to_ascii_lowercase().as_str() {
                    "name" | "label" | "vm" => (Some(Field::Name), value),
                    "uuid" => (Some(Field::Uuid), value),
                    "description" | "desc" => (Some(Field::Description), value),
                    "os" | "os_type" => (Some(Field::Os), value),
                    "kind" | "type" => (Some(Field::Kind), value),
                    "connection" | "pool" => (Some(Field::Connection), value),
                    "power_state" | "power" | "state" => (Some(Field::PowerState), value),
                    "host" | "hostname" => (Some(Field::Host), value),
                    "tag" | "tags" => (Some(Field::Tag), value),
                    "vcpus" | "cpu" | "cpus" => (Some(Field::Vcpus), value),
                    "memory" | "mem" => (Some(Field::Memory), value),
                    _ => (None, token.as_str()),
                },
                None => (None, token.as_str()),
            };
            let value = value.trim();
            if !value.is_empty() {
                let (comparison, numeric_value) = split_comparison(value);
                let number = matches!(field, Some(Field::Vcpus | Field::Memory))
                    .then(|| parse_number(numeric_value))
                    .flatten();
                terms.push(Term {
                    negated,
                    field,
                    value: value.to_owned(),
                    comparison,
                    number,
                });
            }
        }
        Self { terms }
    }

    pub fn matches(&self, vm: &VirtualMachine, connection_name: &str) -> bool {
        self.terms
            .iter()
            .all(|term| term.matches(vm, connection_name))
    }
}

impl Term {
    fn matches(&self, vm: &VirtualMachine, connection_name: &str) -> bool {
        let needle = self.value.to_lowercase();
        let matched = match self.field {
            Some(Field::Name) => vm.name.to_lowercase().contains(&needle),
            Some(Field::Uuid) => vm.uuid.to_lowercase().contains(&needle),
            Some(Field::Description) => vm.description.to_lowercase().contains(&needle),
            Some(Field::Os) => vm.os.to_lowercase().contains(&needle),
            Some(Field::Kind) => vm.kind.eq_ignore_ascii_case(&needle),
            Some(Field::Connection) => connection_name.to_lowercase().contains(&needle),
            Some(Field::PowerState) => power_matches(&vm.power_state, &needle),
            Some(Field::Host) => vm.host_name.to_lowercase().contains(&needle),
            Some(Field::Tag) => vm.tags.iter().any(|tag| tag.to_lowercase() == needle),
            Some(Field::Vcpus) => self
                .number
                .zip(self.comparison)
                .is_some_and(|(number, comparison)| comparison.matches(vm.vcpus, number)),
            Some(Field::Memory) => self
                .number
                .zip(self.comparison)
                .is_some_and(|(number, comparison)| comparison.matches(vm.memory, number)),
            None => {
                vm.name.to_lowercase().contains(&needle)
                    || vm.uuid.to_lowercase().contains(&needle)
                    || vm.host_name.to_lowercase().contains(&needle)
                    || connection_name.to_lowercase().contains(&needle)
                    || vm
                        .tags
                        .iter()
                        .any(|tag| tag.to_lowercase().contains(&needle))
                    || vm.description.to_lowercase().contains(&needle)
            }
        };
        matched != self.negated
    }
}

fn split_comparison(value: &str) -> (Option<Comparison>, &str) {
    for (prefix, comparison) in [
        (">=", Comparison::Ge),
        ("<=", Comparison::Le),
        (">", Comparison::Gt),
        ("<", Comparison::Lt),
        ("=", Comparison::Eq),
    ] {
        if let Some(rest) = value.strip_prefix(prefix) {
            return (Some(comparison), rest.trim());
        }
    }
    (Some(Comparison::Eq), value)
}

fn parse_number(value: &str) -> Option<u64> {
    let value = value.trim().to_ascii_lowercase();
    for (suffix, multiplier) in [
        ("gib", 1024u64 * 1024 * 1024),
        ("gb", 1024u64 * 1024 * 1024),
        ("g", 1024u64 * 1024 * 1024),
        ("mib", 1024u64 * 1024),
        ("mb", 1024u64 * 1024),
        ("m", 1024u64 * 1024),
        ("kib", 1024u64),
        ("kb", 1024u64),
        ("k", 1024u64),
    ] {
        if let Some(number) = value.strip_suffix(suffix) {
            return number.parse::<u64>().ok()?.checked_mul(multiplier);
        }
    }
    value.parse().ok()
}

fn power_matches(state: &str, needle: &str) -> bool {
    match needle {
        "running" | "up" => state.eq_ignore_ascii_case("Running"),
        "halted" | "stopped" | "off" => {
            state.eq_ignore_ascii_case("Halted") || state.eq_ignore_ascii_case("Stopped")
        }
        "paused" => state.eq_ignore_ascii_case("Paused"),
        "suspended" => state.eq_ignore_ascii_case("Suspended"),
        "any" | "all" | "*" => true,
        _ => state.eq_ignore_ascii_case(needle),
    }
}

fn tokenize(input: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut quoted = false;
    let mut escaped = false;
    for character in input.chars() {
        if escaped {
            current.push(character);
            escaped = false;
            continue;
        }
        match character {
            '\\' if quoted => escaped = true,
            '"' => quoted = !quoted,
            character if character.is_whitespace() && !quoted => {
                if !current.is_empty() {
                    tokens.push(std::mem::take(&mut current));
                }
            }
            character => current.push(character),
        }
    }
    if escaped {
        current.push('\\');
    }
    if !current.is_empty() {
        tokens.push(current);
    }
    tokens
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vm() -> VirtualMachine {
        VirtualMachine {
            reference: "ref".into(),
            kind: "vm".into(),
            allowed_operations: Vec::new(),
            host_name: "host-a".into(),
            resident_on: "host-ref".into(),
            snapshot_of: String::new(),
            snapshot_time: String::new(),
            disk_count: 0,
            interface_count: 0,
            uuid: "1234-abcd".into(),
            name: "web server".into(),
            description: "production frontend".into(),
            power_state: "Running".into(),
            vcpus: 2,
            memory: 8 * 1024 * 1024 * 1024,
            os: "linux".into(),
            tags: vec!["prod".into(), "frontend".into()],
        }
    }

    #[test]
    fn parses_fields_quotes_negation_and_free_text() {
        let query = SearchQuery::parse(r#"name:"web server" power_state:stopped -tag:test prod"#);
        assert_eq!(query.terms.len(), 4);
        assert_eq!(query.terms[0].field, Some(Field::Name));
        assert_eq!(query.terms[0].value, "web server");
        assert_eq!(query.terms[1].field, Some(Field::PowerState));
        assert!(query.terms[2].negated);
        assert_eq!(query.terms[2].field, Some(Field::Tag));
        assert_eq!(query.terms[3].field, None);
        let numeric = SearchQuery::parse("vcpus:>=4 memory:<8g");
        assert_eq!(numeric.terms[0].comparison, Some(Comparison::Ge));
        assert_eq!(numeric.terms[0].number, Some(4));
        assert_eq!(numeric.terms[1].comparison, Some(Comparison::Lt));
        assert_eq!(numeric.terms[1].number, Some(8 * 1024 * 1024 * 1024));
    }

    #[test]
    fn filters_name_power_host_and_tag() {
        let vm = vm();
        assert!(SearchQuery::parse("name:web").matches(&vm, "conn"));
        assert!(SearchQuery::parse("power_state:running").matches(&vm, "conn"));
        assert!(SearchQuery::parse("power:up").matches(&vm, "conn"));
        assert!(SearchQuery::parse("host:host-a").matches(&vm, "conn"));
        assert!(SearchQuery::parse("tag:prod").matches(&vm, "conn"));
        assert!(!SearchQuery::parse("tag:pro").matches(&vm, "conn"));
        assert!(SearchQuery::parse("uuid:1234").matches(&vm, "conn"));
        assert!(SearchQuery::parse("description:frontend").matches(&vm, "conn"));
        assert!(SearchQuery::parse("os:linux").matches(&vm, "conn"));
        assert!(SearchQuery::parse("kind:vm").matches(&vm, "conn"));
        assert!(SearchQuery::parse("connection:conn").matches(&vm, "conn"));
        assert!(SearchQuery::parse("vcpus:>=2").matches(&vm, "conn"));
        assert!(!SearchQuery::parse("vcpus:>2").matches(&vm, "conn"));
        assert!(SearchQuery::parse("memory:>=8g").matches(&vm, "conn"));
        assert!(!SearchQuery::parse("memory:>8g").matches(&vm, "conn"));
        assert!(!SearchQuery::parse("host:host-b").matches(&vm, "conn"));
        assert!(!SearchQuery::parse("-tag:prod").matches(&vm, "conn"));
        assert!(SearchQuery::parse("tag:prod -tag:test").matches(&vm, "conn"));
    }

    #[test]
    fn free_text_searches_name_uuid_host_connection_description_and_tags() {
        let vm = vm();
        for text in ["web", "1234", "host-a", "conn", "frontend", "production"] {
            assert!(
                SearchQuery::parse(text).matches(&vm, "conn"),
                "{text} should match"
            );
        }
        assert!(!SearchQuery::parse("missing").matches(&vm, "conn"));
    }

    #[test]
    fn empty_query_accepts_everything() {
        assert!(SearchQuery::parse("   ").terms.is_empty());
        assert!(SearchQuery::parse("").matches(&vm(), "any"));
    }
}
