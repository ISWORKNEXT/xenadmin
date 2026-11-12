//! XenSearch-style boolean query parser for VM and template lists.
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
    Folder,
    Custom,
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
    custom_key: Option<String>,
    case_sensitive: bool,
    negated: bool,
    field: Option<Field>,
    value: String,
    comparison: Option<Comparison>,
    number: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Expr {
    Term(Term),
    Not(Box<Expr>),
    And(Box<Expr>, Box<Expr>),
    Or(Box<Expr>, Box<Expr>),
}

impl Expr {
    fn matches(&self, vm: &VirtualMachine, connection_name: &str) -> bool {
        match self {
            Self::Term(term) => term.matches(vm, connection_name),
            Self::Not(expr) => !expr.matches(vm, connection_name),
            Self::And(left, right) => {
                left.matches(vm, connection_name) && right.matches(vm, connection_name)
            }
            Self::Or(left, right) => {
                left.matches(vm, connection_name) || right.matches(vm, connection_name)
            }
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SearchQuery {
    expr: Option<Expr>,
}

impl SearchQuery {
    pub fn parse(input: &str) -> Self {
        let tokens = tokenize(input);
        if tokens.is_empty() {
            return Self::default();
        }
        let expr = Parser::new(tokens.clone())
            .parse()
            .or_else(|| flat_and(&tokens));
        Self { expr }
    }

    pub fn matches(&self, vm: &VirtualMachine, connection_name: &str) -> bool {
        self.expr
            .as_ref()
            .is_none_or(|expr| expr.matches(vm, connection_name))
    }
}

impl Term {
    fn parse(mut token: String) -> Option<Self> {
        let negated = token.starts_with('-') && token.len() > 1;
        if negated {
            token.remove(0);
        }
        let mut custom_key = None;
        let mut case_sensitive = false;
        let (field, value) = match token.split_once(':') {
            Some((field, value)) => {
                let field = if let Some(field) = field.strip_suffix(".cs") {
                    case_sensitive = true;
                    field
                } else {
                    field
                };
                match field.to_ascii_lowercase().as_str() {
                    "name" | "label" | "vm" => (Some(Field::Name), value),
                    "uuid" => (Some(Field::Uuid), value),
                    "description" | "desc" => (Some(Field::Description), value),
                    "os" | "os_type" => (Some(Field::Os), value),
                    "kind" | "type" => (Some(Field::Kind), value),
                    "connection" | "pool" => (Some(Field::Connection), value),
                    "power_state" | "power" | "state" => (Some(Field::PowerState), value),
                    "host" | "hostname" => (Some(Field::Host), value),
                    "tag" | "tags" => (Some(Field::Tag), value),
                    "folder" => (Some(Field::Folder), value),
                    _ if field.starts_with("field.") => {
                        custom_key = Some(field[6..].into());
                        (Some(Field::Custom), value)
                    }
                    "vcpus" | "cpu" | "cpus" => (Some(Field::Vcpus), value),
                    "memory" | "mem" => (Some(Field::Memory), value),
                    _ => (None, token.as_str()),
                }
            }
            None => (None, token.as_str()),
        };
        let value = value.trim();
        if value.is_empty() {
            return None;
        }
        let (comparison, numeric_value) = split_comparison(value);
        let number = matches!(field, Some(Field::Vcpus | Field::Memory))
            .then(|| parse_number(numeric_value))
            .flatten();
        Some(Self {
            custom_key,
            case_sensitive,
            negated,
            field,
            value: value.to_owned(),
            comparison,
            number,
        })
    }

    fn matches(&self, vm: &VirtualMachine, connection_name: &str) -> bool {
        let needle = self.value.to_lowercase();
        let string_matches = |actual: &str| {
            let actual = if self.case_sensitive {
                actual.to_owned()
            } else {
                actual.to_lowercase()
            };
            let needle = if self.case_sensitive {
                self.value.clone()
            } else {
                needle.clone()
            };
            if let Some(value) = needle.strip_prefix('=') {
                actual == value
            } else if let Some(value) = needle.strip_prefix('^') {
                actual.starts_with(value)
            } else if let Some(value) = needle.strip_prefix('$') {
                actual.ends_with(value)
            } else {
                actual.contains(&needle)
            }
        };
        let matched = match self.field {
            Some(Field::Name) => string_matches(&vm.name),
            Some(Field::Uuid) => vm.uuid.to_lowercase().contains(&needle),
            Some(Field::Description) => vm.description.to_lowercase().contains(&needle),
            Some(Field::Os) => string_matches(&vm.os),
            Some(Field::Kind) => match needle.as_str() {
                "default_template" => vm.kind == "template" && vm.default_template,
                "user_template" => vm.kind == "template" && !vm.default_template,
                "template" => vm.kind == "template",
                "storage" => matches!(vm.kind.as_str(), "storage" | "local_sr" | "remote_sr"),
                _ => vm.kind.eq_ignore_ascii_case(&needle),
            },
            Some(Field::Connection) => connection_name.to_lowercase().contains(&needle),
            Some(Field::PowerState) => power_matches(&vm.power_state, &needle),
            Some(Field::Host) => vm.host_name.to_lowercase().contains(&needle),
            Some(Field::Folder) => string_matches(&vm.folder),
            Some(Field::Custom) => vm
                .custom_fields
                .iter()
                .find(|(key, _)| Some(key) == self.custom_key.as_ref())
                .is_some_and(|(_, value)| string_matches(value)),
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

#[derive(Clone, Debug, PartialEq, Eq)]
enum Token {
    Word(String),
    LeftParen,
    RightParen,
}

impl Token {
    fn is_word(&self, expected: &str) -> bool {
        matches!(self, Self::Word(word) if word.eq_ignore_ascii_case(expected))
    }

    fn starts_primary(&self) -> bool {
        match self {
            Self::Word(word) => !word.eq_ignore_ascii_case("OR"),
            Self::LeftParen => true,
            Self::RightParen => false,
        }
    }
}

struct Parser {
    tokens: Vec<Token>,
    position: usize,
}

impl Parser {
    fn new(tokens: Vec<Token>) -> Self {
        Self {
            tokens,
            position: 0,
        }
    }

    fn parse(mut self) -> Option<Expr> {
        let expr = self.parse_or()?;
        (self.position == self.tokens.len()).then_some(expr)
    }

    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.position)
    }

    fn parse_or(&mut self) -> Option<Expr> {
        let mut left = self.parse_and()?;
        while self.peek().is_some_and(|token| token.is_word("OR")) {
            self.position += 1;
            let right = self.parse_and()?;
            left = Expr::Or(Box::new(left), Box::new(right));
        }
        Some(left)
    }

    fn parse_and(&mut self) -> Option<Expr> {
        let mut left = self.parse_unary()?;
        loop {
            if self.peek().is_some_and(|token| token.is_word("AND")) {
                self.position += 1;
                let right = self.parse_unary()?;
                left = Expr::And(Box::new(left), Box::new(right));
            } else if self.peek().is_some_and(Token::starts_primary) {
                // Adjacent terms are an implicit AND: `name:web tag:prod`.
                let right = self.parse_unary()?;
                left = Expr::And(Box::new(left), Box::new(right));
            } else {
                break;
            }
        }
        Some(left)
    }

    fn parse_unary(&mut self) -> Option<Expr> {
        if self.peek().is_some_and(|token| token.is_word("NOT")) {
            self.position += 1;
            return Some(Expr::Not(Box::new(self.parse_unary()?)));
        }
        match self.tokens.get(self.position)?.clone() {
            Token::LeftParen => {
                self.position += 1;
                let expr = self.parse_or()?;
                match self.tokens.get(self.position) {
                    Some(Token::RightParen) => {
                        self.position += 1;
                        Some(expr)
                    }
                    _ => None,
                }
            }
            Token::Word(word) => {
                self.position += 1;
                Term::parse(word).map(Expr::Term)
            }
            Token::RightParen => None,
        }
    }
}

fn flat_and(tokens: &[Token]) -> Option<Expr> {
    let mut expr: Option<Expr> = None;
    for token in tokens {
        let Token::Word(word) = token else {
            continue;
        };
        if ["AND", "OR", "NOT"]
            .iter()
            .any(|operator| word.eq_ignore_ascii_case(operator))
        {
            continue;
        }
        let Some(term) = Term::parse(word.clone()) else {
            continue;
        };
        let term = Expr::Term(term);
        expr = Some(match expr {
            Some(left) => Expr::And(Box::new(left), Box::new(term)),
            None => term,
        });
    }
    expr
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

fn tokenize(input: &str) -> Vec<Token> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut quoted = false;
    let mut escaped = false;
    let flush = |current: &mut String, tokens: &mut Vec<Token>| {
        if !current.is_empty() {
            tokens.push(Token::Word(std::mem::take(current)));
        }
    };
    for character in input.chars() {
        if escaped {
            current.push(character);
            escaped = false;
            continue;
        }
        match character {
            '\\' if quoted => escaped = true,
            '"' => quoted = !quoted,
            '(' | ')' if !quoted => {
                flush(&mut current, &mut tokens);
                tokens.push(if character == '(' {
                    Token::LeftParen
                } else {
                    Token::RightParen
                });
            }
            character if character.is_whitespace() && !quoted => {
                flush(&mut current, &mut tokens);
            }
            character => current.push(character),
        }
    }
    if escaped {
        current.push('\\');
    }
    flush(&mut current, &mut tokens);
    tokens
}

#[cfg(test)]
mod tests {
    use super::*;

    pub(super) fn sample_vm(name: &str, tags: &[&str], power: &str) -> VirtualMachine {
        VirtualMachine {
            instant_template: false,
            cbt_enabled: false,
            default_template: false,
            reference: "ref".into(),
            kind: "vm".into(),
            allowed_operations: Vec::new(),
            host_name: "host-a".into(),
            resident_on: "host-ref".into(),
            snapshot_of: String::new(),
            snapshot_time: String::new(),
            parent: String::new(),
            folder: String::new(),
            custom_fields: Vec::new(),
            disk_count: 0,
            interface_count: 0,
            uuid: "1234-abcd".into(),
            name: name.into(),
            description: "production frontend".into(),
            power_state: power.into(),
            vcpus: 2,
            memory: 8 * 1024 * 1024 * 1024,
            os: "linux".into(),
            tags: tags.iter().map(|tag| (*tag).to_owned()).collect(),
        }
    }

    #[test]
    fn supports_and_or_not_parentheses_and_precedence() {
        let web = sample_vm("web", &["prod"], "Running");
        let db = sample_vm("db", &[], "Halted");
        assert!(SearchQuery::parse("name:web OR name:db").matches(&web, "conn"));
        assert!(SearchQuery::parse("name:web OR name:db").matches(&db, "conn"));
        assert!(!SearchQuery::parse("name:web AND name:db").matches(&web, "conn"));
        assert!(SearchQuery::parse("name:web name:web").matches(&web, "conn"));
        assert!(SearchQuery::parse("NOT name:db").matches(&web, "conn"));
        assert!(!SearchQuery::parse("NOT name:db").matches(&db, "conn"));
        // AND binds tighter than OR.
        assert!(SearchQuery::parse("name:web OR name:db AND tag:prod").matches(&web, "conn"));
        assert!(!SearchQuery::parse("name:web OR name:db AND tag:prod").matches(&db, "conn"));
        // Parentheses override precedence.
        assert!(
            SearchQuery::parse("(name:web OR name:db) AND power_state:halted").matches(&db, "conn")
        );
        assert!(
            !SearchQuery::parse("(name:web OR name:db) AND power_state:halted")
                .matches(&web, "conn")
        );
    }

    #[test]
    fn filters_fields_and_comparators() {
        let vm = sample_vm("web server", &["prod", "frontend"], "Running");
        assert!(SearchQuery::parse("name:web").matches(&vm, "conn"));
        assert!(SearchQuery::parse("power_state:running").matches(&vm, "conn"));
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
    fn quoted_values_keep_parentheses_and_spaces() {
        let vm = sample_vm("web (prod)", &[], "Running");
        assert!(SearchQuery::parse(r#"name:"web (prod)""#).matches(&vm, "conn"));
    }

    #[test]
    fn free_text_searches_name_uuid_host_connection_description_and_tags() {
        let vm = sample_vm("web server", &["prod", "frontend"], "Running");
        for text in ["web", "1234", "host-a", "conn", "frontend", "production"] {
            assert!(
                SearchQuery::parse(text).matches(&vm, "conn"),
                "{text} should match"
            );
        }
        assert!(!SearchQuery::parse("missing").matches(&vm, "conn"));
    }

    #[test]
    fn malformed_queries_fall_back_to_flat_and_without_panicking() {
        let vm = sample_vm("web", &["prod"], "Running");
        assert!(SearchQuery::parse("(name:web").matches(&vm, "conn"));
        assert!(SearchQuery::parse("name:web )").matches(&vm, "conn"));
        assert!(SearchQuery::parse("() name:web").matches(&vm, "conn"));
        assert!(SearchQuery::parse("").matches(&vm, "any"));
    }
}

/// Translate supported XenCenter query XML without dropping unknown conditions.
pub fn import_upstream(query: roxmltree::Node<'_, '_>) -> Result<String, String> {
    fn translate(n: roxmltree::Node<'_, '_>) -> Result<String, String> {
        let attr = |key| {
            n.attribute(key)
                .ok_or_else(|| format!("Missing {key} in {}", n.tag_name().name()))
        };
        let quoted =
            |value: &str| format!("\"{}\"", value.replace('\\', "\\\\").replace('\"', "\\\""));
        let property = || -> Result<&str, String> {
            Ok(match attr("property")? {
                "label" | "name_label" => "name",
                "folder" => "folder",
                "tags" => "tag",
                "power_state" => "power",
                "memory" | "memory_dynamic_max" | "memory_static_max" => "memory",
                "os_name" => "os",
                "type" => "kind",
                other => return Err(format!("Unsupported search property: {other}")),
            })
        };
        match n.tag_name().name() {
            "Query" => {
                let parts = n
                    .children()
                    .filter(|n| n.is_element())
                    .map(translate)
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(format!(
                    "({})",
                    parts
                        .into_iter()
                        .filter(|s| !s.is_empty())
                        .collect::<Vec<_>>()
                        .join(" AND ")
                ))
            }
            "QueryScope" => {
                let kinds = n
                    .children()
                    .filter(|n| n.is_element())
                    .map(|n| match n.tag_name().name() {
                        "VM" => Ok("kind:vm"),
                        "Server" => Ok("kind:host"),
                        "Pool" => Ok("kind:pool"),
                        "RemoteSR" => Ok("kind:remote_sr"),
                        "LocalSR" => Ok("kind:local_sr"),
                        "VDI" => Ok("kind:disk"),
                        "Network" => Ok("kind:network"),
                        "Template" => Ok("kind:template"),
                        "UserTemplate" => Ok("kind:user_template"),
                        "DefaultTemplate" => Ok("kind:default_template"),
                        "Snapshot" => Ok("kind:snapshot"),
                        other => Err(format!("Unsupported search scope: {other}")),
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                if kinds.is_empty() {
                    Ok("kind:=__none__".into())
                } else {
                    Ok(format!("({})", kinds.join(" OR ")))
                }
            }
            "GroupQuery" => {
                let kind = attr("type")?;
                let join = match kind {
                    "And" => " AND ",
                    "Or" | "Nor" => " OR ",
                    _ => return Err(format!("Unsupported group: {kind}")),
                };
                let children = n
                    .children()
                    .filter(|n| n.is_element())
                    .map(translate)
                    .collect::<Result<Vec<_>, _>>()?;
                if children.is_empty() {
                    return Err("Empty query group is not supported.".into());
                }
                Ok(format!(
                    "{}({})",
                    if kind == "Nor" { "NOT " } else { "" },
                    children.join(join)
                ))
            }
            "XenModelObjectPropertyQuery" if attr("property")? == "folder" => {
                let path = attr("uuid")?;
                Ok(format!(
                    "{}folder:{}",
                    if n.attribute("equals") == Some("no") {
                        "NOT "
                    } else {
                        ""
                    },
                    quoted(&format!("={path}"))
                ))
            }
            "RecursiveXMOPropertyQuery" if attr("property")? == "folder" => {
                let child = n
                    .children()
                    .find(|n| n.is_element())
                    .ok_or("Missing folder condition")?;
                if child.tag_name().name() != "StringPropertyQuery"
                    || child.attribute("property") != Some("uuid")
                {
                    return Err("Unsupported recursive folder query".into());
                }
                let mode = match child.attribute("type") {
                    Some("exactmatch") => "=",
                    Some("contains") => "",
                    Some("startswith") => "^",
                    Some("endswith") => "$",
                    _ => return Err("Unsupported folder comparison".into()),
                };
                Ok(format!(
                    "folder:{}",
                    quoted(&format!(
                        "{mode}{}",
                        child.attribute("query").ok_or("Missing folder query")?
                    ))
                ))
            }
            "StringPropertyQuery" | "CustomFieldQuery" => {
                let field = if n.tag_name().name() == "CustomFieldQuery" {
                    let def = n
                        .children()
                        .find(|n| n.tag_name().name() == "CustomFieldDefinition")
                        .ok_or("Missing custom field definition")?;
                    format!(
                        "field.{}",
                        def.attribute("name").ok_or("Missing field name")?
                    )
                } else {
                    property()?.to_owned()
                };
                let mode = attr("type")?;
                let prefix = match mode {
                    "exactmatch" => "=",
                    "contains" | "notcontain" => "",
                    "startswith" => "^",
                    "endswith" => "$",
                    _ => return Err(format!("Unsupported string comparison: {mode}")),
                };
                let sensitive = if n.attribute("casesensitive") == Some("yes") {
                    ".cs"
                } else {
                    ""
                };
                Ok(format!(
                    "{}{}:{}",
                    if mode == "notcontain" { "NOT " } else { "" },
                    quoted(&format!("{field}{sensitive}")),
                    quoted(&format!("{prefix}{}", attr("query")?))
                ))
            }
            "EnumPropertyQuery" | "ValuePropertyQuery" | "StringListContainsQuery" => {
                let field = property()?;
                let mut value = attr("query")?.to_owned();
                if field == "kind" {
                    value = match value.as_str() {
                        "VM" => "vm",
                        "UserTemplate" => "user_template",
                        "DefaultTemplate" => "default_template",
                        "Server" => "host",
                        "Pool" => "pool",
                        "RemoteSR" => "remote_sr",
                        "LocalSR" => "local_sr",
                        "VDI" => "disk",
                        "Network" => "network",
                        "Snapshot" => "snapshot",
                        other => return Err(format!("Unsupported object type: {other}")),
                    }
                    .into();
                }
                let negative =
                    n.attribute("equals") == Some("no") || n.attribute("contains") == Some("no");
                Ok(format!(
                    "{}{}:{}",
                    if negative { "NOT " } else { "" },
                    field,
                    quoted(&if field == "tag" || field == "power" || field == "kind" {
                        value
                    } else {
                        format!("={value}")
                    })
                ))
            }
            "IntPropertyQuery" => {
                if property()? != "memory" {
                    return Err("Unsupported numeric property".into());
                }
                let value = attr("query")?
                    .parse::<u64>()
                    .map_err(|_| "Invalid numeric query")?;
                let op = match attr("type")? {
                    "exactmatch" => "=",
                    "gt" => ">",
                    "lt" => "<",
                    other => return Err(format!("Unsupported numeric comparison: {other}")),
                };
                Ok(format!("memory:{op}{value}"))
            }
            other => Err(format!("Unsupported upstream condition: {other}")),
        }
    }
    translate(query)
}

/// Export the compatible subset; callers retain native queries if not representable.
pub fn export_upstream(input: &str) -> Option<String> {
    fn xml(expr: &Expr) -> Option<String> {
        let esc = |s: &str| quick_xml::escape::escape(s).into_owned();
        match expr {
            Expr::And(a, b) | Expr::Or(a, b) => Some(format!(
                "<GroupQuery type=\"{}\">{}{}</GroupQuery>",
                if matches!(expr, Expr::And(..)) {
                    "And"
                } else {
                    "Or"
                },
                xml(a)?,
                xml(b)?
            )),
            Expr::Not(child) => Some(format!(
                "<GroupQuery type=\"Nor\">{}</GroupQuery>",
                xml(child)?
            )),
            Expr::Term(term) => {
                let property = match term.field {
                    Some(Field::Name) => "label",
                    Some(Field::Folder) => "folder",
                    Some(Field::Tag) => "tags",
                    Some(Field::Memory) => "memory",
                    Some(Field::Os) => "os_name",
                    Some(Field::PowerState) => "power_state",
                    Some(Field::Kind) => "type",
                    Some(Field::Custom) => "",
                    _ => return None,
                };
                let value = &term.value;
                let result = match term.field {
                    Some(Field::Memory) => format!(
                        "<IntPropertyQuery property=\"memory\" type=\"{}\" query=\"{}\"/>",
                        match term.comparison? {
                            Comparison::Eq => "exactmatch",
                            Comparison::Gt => "gt",
                            Comparison::Lt => "lt",
                            _ => return None,
                        },
                        term.number?
                    ),
                    Some(Field::Tag) => format!(
                        "<StringListContainsQuery property=\"tags\" contains=\"yes\" query=\"{}\"/>",
                        esc(value)
                    ),
                    Some(Field::PowerState | Field::Kind) => {
                        let value = if term.field == Some(Field::Kind) {
                            match value.to_lowercase().as_str() {
                                "vm" => "VM",
                                "template" => return None,
                                "snapshot" => "Snapshot",
                                _ => return None,
                            }
                        } else {
                            match value.to_lowercase().as_str() {
                                "running" => "Running",
                                "halted" | "stopped" | "off" => "Halted",
                                "paused" => "Paused",
                                "suspended" => "Suspended",
                                _ => return None,
                            }
                        };
                        format!(
                            "<EnumPropertyQuery property=\"{property}\" query=\"{value}\" equals=\"yes\"/>"
                        )
                    }
                    _ => {
                        let (mode, value) = if let Some(v) = value.strip_prefix('=') {
                            ("exactmatch", v)
                        } else if let Some(v) = value.strip_prefix('^') {
                            ("startswith", v)
                        } else if let Some(v) = value.strip_prefix('$') {
                            ("endswith", v)
                        } else {
                            ("contains", value.as_str())
                        };
                        if term.field == Some(Field::Folder) {
                            format!(
                                "<RecursiveXMOPropertyQuery property=\"folder\"><StringPropertyQuery property=\"uuid\" type=\"{mode}\" query=\"{}\" casesensitive=\"{}\"/></RecursiveXMOPropertyQuery>",
                                esc(value),
                                if term.case_sensitive { "yes" } else { "no" }
                            )
                        } else if term.field == Some(Field::Custom) {
                            format!(
                                "<CustomFieldQuery type=\"{mode}\" query=\"{}\"><CustomFieldDefinition name=\"{}\" type=\"String\" defaultValue=\"\"/></CustomFieldQuery>",
                                esc(value),
                                esc(term.custom_key.as_ref()?)
                            )
                        } else {
                            format!(
                                "<StringPropertyQuery property=\"{property}\" type=\"{mode}\" query=\"{}\" casesensitive=\"{}\"/>",
                                esc(value),
                                if term.case_sensitive { "yes" } else { "no" }
                            )
                        }
                    }
                };
                Some(if term.negated {
                    format!("<GroupQuery type=\"Nor\">{result}</GroupQuery>")
                } else {
                    result
                })
            }
        }
    }
    let filter = if input.trim().is_empty() {
        String::new()
    } else {
        xml(&Parser::new(tokenize(input)).parse()?)?
    };
    Some(format!(
        "<Query><QueryScope><VM/><Snapshot/><UserTemplate/><DefaultTemplate/></QueryScope>{filter}</Query>"
    ))
}

#[cfg(test)]
mod xml_tests {
    use super::*;
    #[test]
    fn upstream_xml_translates_conditions_and_boolean_groups() {
        let xml = r#"<Query><QueryScope><VM/></QueryScope><GroupQuery type="And"><StringPropertyQuery property="label" query="web &amp; prod" type="exactmatch" casesensitive="yes"/><StringListContainsQuery property="tags" query="prod" contains="yes"/><IntPropertyQuery property="memory" query="1024" type="gt"/><CustomFieldQuery type="contains" query="team"><CustomFieldDefinition name="Owner" type="String"/></CustomFieldQuery></GroupQuery></Query>"#;
        let doc = roxmltree::Document::parse(xml).unwrap();
        let query = import_upstream(doc.root_element()).unwrap();
        let mut vm = super::tests::sample_vm("web & prod", &["prod"], "Running");
        vm.custom_fields.push(("Owner".into(), "team-a".into()));
        assert!(SearchQuery::parse(&query).matches(&vm, "conn"), "{query}");
        vm.name = "Web & prod".into();
        assert!(!SearchQuery::parse(&query).matches(&vm, "conn"));
        assert!(
            import_upstream(
                roxmltree::Document::parse("<Unknown/>")
                    .unwrap()
                    .root_element()
            )
            .is_err()
        );
    }
    #[test]
    fn upstream_export_round_trip_matches_behavior() {
        for query in [
            "name:web tag:prod",
            "NOT power:halted",
            "memory:>1024 OR folder:=/one",
            r#"field.Owner:"=Team & ops""#,
        ] {
            let xml = export_upstream(query).unwrap();
            let doc = roxmltree::Document::parse(&xml).unwrap();
            let imported = import_upstream(doc.root_element()).unwrap();
            for name in ["web", "db"] {
                let mut vm = super::tests::sample_vm(name, &["prod"], "Running");
                vm.folder = "/one".into();
                vm.custom_fields.push(("Owner".into(), "Team & ops".into()));
                assert_eq!(
                    SearchQuery::parse(query).matches(&vm, "conn"),
                    SearchQuery::parse(&imported).matches(&vm, "conn"),
                    "{query} => {imported}"
                );
            }
        }
    }
}
