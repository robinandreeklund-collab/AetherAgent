// ─── CSS Selector Matching via Servo's `selectors` crate ─────────────────────
//
// Riktig implementation som använder selectors 0.36 (samma engine som Stylo/Firefox).
// Implementerar selectors::Element trait för ArenaDom-noder.
// Stödjer alla CSS-selektorer: :has(), :is(), :where(), :not(), alla combinators, etc.

use crate::arena_dom::{ArenaDom, NodeKey, NodeType};
use cssparser::{CowRcStr, SourceLocation, ToCss};
use selectors::attr::{
    AttrSelectorOperation, AttrSelectorOperator, CaseSensitivity, NamespaceConstraint,
};
use selectors::context::{MatchingContext, MatchingMode, QuirksMode, SelectorCaches};
use selectors::matching::ElementSelectorFlags;
use selectors::matching::{MatchingForInvalidation, NeedsSelectorFlags};
use selectors::parser::{self as sel_parser, ParseRelative, SelectorImpl, SelectorList};
use selectors::OpaqueElement;
use std::fmt;

// ─── SelectorImpl: Definierar typerna som selectors-craten arbetar med ───────

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct ArenaSelectorImpl;

/// Enkel strängtyp för attributvärden, identifiers, etc.
#[derive(Clone, Debug, Eq, PartialEq, Hash, Default)]
pub(super) struct ArenaStr(String);

impl AsRef<str> for ArenaStr {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl std::borrow::Borrow<str> for ArenaStr {
    fn borrow(&self) -> &str {
        &self.0
    }
}

impl precomputed_hash::PrecomputedHash for ArenaStr {
    fn precomputed_hash(&self) -> u32 {
        let mut h: u32 = 5381;
        for byte in self.0.bytes() {
            h = h.wrapping_mul(33).wrapping_add(byte as u32);
        }
        h
    }
}

impl ToCss for ArenaStr {
    fn to_css<W: fmt::Write>(&self, dest: &mut W) -> fmt::Result {
        cssparser::serialize_identifier(&self.0, dest)
    }
}

impl From<&str> for ArenaStr {
    fn from(s: &str) -> Self {
        ArenaStr(s.to_string())
    }
}

/// Non-tree-structural pseudo-class — :hover, :focus, :checked, etc.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum ArenaPseudoClass {
    Hover,
    Focus,
    Active,
    Checked,
    Disabled,
    Enabled,
    ReadOnly,
    ReadWrite,
    Link,
    Visited,
    AnyLink,
    Target,
    FocusVisible,
    FocusWithin,
    Indeterminate,
    PlaceholderShown,
    Default,
    Defined,
    Required,
    Optional,
    Valid,
    Invalid,
    Scope,
}

impl selectors::parser::NonTSPseudoClass for ArenaPseudoClass {
    type Impl = ArenaSelectorImpl;
    fn is_active_or_hover(&self) -> bool {
        matches!(self, ArenaPseudoClass::Hover | ArenaPseudoClass::Active)
    }
    fn is_user_action_state(&self) -> bool {
        matches!(
            self,
            ArenaPseudoClass::Hover | ArenaPseudoClass::Active | ArenaPseudoClass::Focus
        )
    }
}

impl ToCss for ArenaPseudoClass {
    fn to_css<W: fmt::Write>(&self, dest: &mut W) -> fmt::Result {
        let s = match self {
            ArenaPseudoClass::Hover => ":hover",
            ArenaPseudoClass::Focus => ":focus",
            ArenaPseudoClass::Active => ":active",
            ArenaPseudoClass::Checked => ":checked",
            ArenaPseudoClass::Disabled => ":disabled",
            ArenaPseudoClass::Enabled => ":enabled",
            ArenaPseudoClass::ReadOnly => ":read-only",
            ArenaPseudoClass::ReadWrite => ":read-write",
            ArenaPseudoClass::Link => ":link",
            ArenaPseudoClass::Visited => ":visited",
            ArenaPseudoClass::AnyLink => ":any-link",
            ArenaPseudoClass::Target => ":target",
            ArenaPseudoClass::FocusVisible => ":focus-visible",
            ArenaPseudoClass::FocusWithin => ":focus-within",
            ArenaPseudoClass::Indeterminate => ":indeterminate",
            ArenaPseudoClass::PlaceholderShown => ":placeholder-shown",
            ArenaPseudoClass::Default => ":default",
            ArenaPseudoClass::Defined => ":defined",
            ArenaPseudoClass::Required => ":required",
            ArenaPseudoClass::Optional => ":optional",
            ArenaPseudoClass::Valid => ":valid",
            ArenaPseudoClass::Invalid => ":invalid",
            ArenaPseudoClass::Scope => ":scope",
        };
        dest.write_str(s)
    }
}

/// Pseudo-element — ::before, ::after, etc. (vi matchar inte pseudo-element i DOM)
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum ArenaPseudoElement {
    Before,
    After,
    FirstLine,
    FirstLetter,
}

impl selectors::parser::PseudoElement for ArenaPseudoElement {
    type Impl = ArenaSelectorImpl;
}

impl ToCss for ArenaPseudoElement {
    fn to_css<W: fmt::Write>(&self, dest: &mut W) -> fmt::Result {
        let s = match self {
            ArenaPseudoElement::Before => "::before",
            ArenaPseudoElement::After => "::after",
            ArenaPseudoElement::FirstLine => "::first-line",
            ArenaPseudoElement::FirstLetter => "::first-letter",
        };
        dest.write_str(s)
    }
}

impl SelectorImpl for ArenaSelectorImpl {
    type ExtraMatchingData<'a> = ();
    type AttrValue = ArenaStr;
    type Identifier = ArenaStr;
    type LocalName = ArenaStr;
    type NamespaceUrl = ArenaStr;
    type NamespacePrefix = ArenaStr;
    type BorrowedNamespaceUrl = str;
    type BorrowedLocalName = str;
    type NonTSPseudoClass = ArenaPseudoClass;
    type PseudoElement = ArenaPseudoElement;
}

// ─── Parser: Parsear CSS-selektorer till selectors-cratens AST ────────────────

struct ArenaParser;

impl<'i> sel_parser::Parser<'i> for ArenaParser {
    type Impl = ArenaSelectorImpl;
    type Error = sel_parser::SelectorParseErrorKind<'i>;

    fn parse_non_ts_pseudo_class(
        &self,
        _location: SourceLocation,
        name: CowRcStr<'i>,
    ) -> Result<ArenaPseudoClass, cssparser::ParseError<'i, Self::Error>> {
        match name.as_ref() {
            "hover" => Ok(ArenaPseudoClass::Hover),
            "focus" => Ok(ArenaPseudoClass::Focus),
            "active" => Ok(ArenaPseudoClass::Active),
            "checked" => Ok(ArenaPseudoClass::Checked),
            "disabled" => Ok(ArenaPseudoClass::Disabled),
            "enabled" => Ok(ArenaPseudoClass::Enabled),
            "read-only" => Ok(ArenaPseudoClass::ReadOnly),
            "read-write" => Ok(ArenaPseudoClass::ReadWrite),
            "link" => Ok(ArenaPseudoClass::Link),
            "visited" => Ok(ArenaPseudoClass::Visited),
            "any-link" => Ok(ArenaPseudoClass::AnyLink),
            "target" => Ok(ArenaPseudoClass::Target),
            "focus-visible" => Ok(ArenaPseudoClass::FocusVisible),
            "focus-within" => Ok(ArenaPseudoClass::FocusWithin),
            "indeterminate" => Ok(ArenaPseudoClass::Indeterminate),
            "placeholder-shown" => Ok(ArenaPseudoClass::PlaceholderShown),
            "default" => Ok(ArenaPseudoClass::Default),
            "defined" => Ok(ArenaPseudoClass::Defined),
            "required" => Ok(ArenaPseudoClass::Required),
            "optional" => Ok(ArenaPseudoClass::Optional),
            "valid" => Ok(ArenaPseudoClass::Valid),
            "invalid" => Ok(ArenaPseudoClass::Invalid),
            "scope" => Ok(ArenaPseudoClass::Scope),
            _ => Err(cssparser::ParseError {
                kind: cssparser::ParseErrorKind::Custom(
                    sel_parser::SelectorParseErrorKind::UnsupportedPseudoClassOrElement(name),
                ),
                location: _location,
            }),
        }
    }

    fn parse_non_ts_functional_pseudo_class<'t>(
        &self,
        name: CowRcStr<'i>,
        _arguments: &mut cssparser::Parser<'i, 't>,
        _after_part: bool,
    ) -> Result<ArenaPseudoClass, cssparser::ParseError<'i, Self::Error>> {
        Err(cssparser::ParseError {
            kind: cssparser::ParseErrorKind::Custom(
                sel_parser::SelectorParseErrorKind::UnsupportedPseudoClassOrElement(name),
            ),
            location: _arguments.current_source_location(),
        })
    }

    fn parse_pseudo_element(
        &self,
        _location: SourceLocation,
        name: CowRcStr<'i>,
    ) -> Result<ArenaPseudoElement, cssparser::ParseError<'i, Self::Error>> {
        match name.as_ref() {
            "before" => Ok(ArenaPseudoElement::Before),
            "after" => Ok(ArenaPseudoElement::After),
            "first-line" => Ok(ArenaPseudoElement::FirstLine),
            "first-letter" => Ok(ArenaPseudoElement::FirstLetter),
            _ => Err(cssparser::ParseError {
                kind: cssparser::ParseErrorKind::Custom(
                    sel_parser::SelectorParseErrorKind::UnsupportedPseudoClassOrElement(name),
                ),
                location: _location,
            }),
        }
    }

    fn parse_is_and_where(&self) -> bool {
        true
    }

    fn parse_has(&self) -> bool {
        true
    }

    fn default_namespace(&self) -> Option<ArenaStr> {
        None
    }

    fn namespace_for_prefix(&self, _prefix: &ArenaStr) -> Option<ArenaStr> {
        None
    }
}

// ─── Element wrapper: kopplar ArenaDom till selectors::Element trait ──────────

#[derive(Clone)]
pub(super) struct ArenaElement<'a> {
    arena: &'a ArenaDom,
    key: NodeKey,
}

impl<'a> fmt::Debug for ArenaElement<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(node) = self.arena.nodes.get(self.key) {
            write!(f, "<{}>", node.tag.as_deref().unwrap_or("?"))
        } else {
            write!(f, "<invalid>")
        }
    }
}

impl<'a> ArenaElement<'a> {
    fn new(arena: &'a ArenaDom, key: NodeKey) -> Self {
        Self { arena, key }
    }

    fn node(&self) -> Option<&'a crate::arena_dom::DomNode> {
        self.arena.nodes.get(self.key)
    }
}

impl<'a> selectors::Element for ArenaElement<'a> {
    type Impl = ArenaSelectorImpl;

    fn opaque(&self) -> OpaqueElement {
        // Stabil pekare: använd arenaslotens adress så samma NodeKey ger samma OpaqueElement.
        // Krävs för att :has() relativa selektorankare ska kunna jämföras korrekt.
        if let Some(node) = self.arena.nodes.get(self.key) {
            OpaqueElement::new(node)
        } else {
            OpaqueElement::new(&())
        }
    }

    fn parent_element(&self) -> Option<Self> {
        let node = self.node()?;
        let parent_key = node.parent?;
        let parent = self.arena.nodes.get(parent_key)?;
        if parent.node_type == NodeType::Element {
            Some(ArenaElement::new(self.arena, parent_key))
        } else {
            None
        }
    }

    fn parent_node_is_shadow_root(&self) -> bool {
        false
    }

    fn containing_shadow_host(&self) -> Option<Self> {
        None
    }

    fn is_pseudo_element(&self) -> bool {
        false
    }

    fn prev_sibling_element(&self) -> Option<Self> {
        let node = self.node()?;
        let parent_key = node.parent?;
        let parent = self.arena.nodes.get(parent_key)?;
        let my_pos = parent.children.iter().position(|&k| k == self.key)?;
        // Sök bakåt efter närmaste element-syskon
        for i in (0..my_pos).rev() {
            let sib_key = parent.children[i];
            if let Some(sib) = self.arena.nodes.get(sib_key) {
                if sib.node_type == NodeType::Element {
                    return Some(ArenaElement::new(self.arena, sib_key));
                }
            }
        }
        None
    }

    fn next_sibling_element(&self) -> Option<Self> {
        let node = self.node()?;
        let parent_key = node.parent?;
        let parent = self.arena.nodes.get(parent_key)?;
        let my_pos = parent.children.iter().position(|&k| k == self.key)?;
        for i in (my_pos + 1)..parent.children.len() {
            let sib_key = parent.children[i];
            if let Some(sib) = self.arena.nodes.get(sib_key) {
                if sib.node_type == NodeType::Element {
                    return Some(ArenaElement::new(self.arena, sib_key));
                }
            }
        }
        None
    }

    fn first_element_child(&self) -> Option<Self> {
        let node = self.node()?;
        for &child_key in &node.children {
            if let Some(child) = self.arena.nodes.get(child_key) {
                if child.node_type == NodeType::Element {
                    return Some(ArenaElement::new(self.arena, child_key));
                }
            }
        }
        None
    }

    fn is_html_element_in_html_document(&self) -> bool {
        // Alla element i ArenaDom anses vara HTML-element i ett HTML-dokument
        true
    }

    fn has_local_name(&self, local_name: &str) -> bool {
        self.node()
            .and_then(|n| n.tag.as_deref())
            .is_some_and(|tag| tag.eq_ignore_ascii_case(local_name))
    }

    fn has_namespace(&self, _ns: &str) -> bool {
        // ArenaDom använder inte namespace — default namespace matchar alltid
        true
    }

    fn is_same_type(&self, other: &Self) -> bool {
        let my_tag = self.node().and_then(|n| n.tag.as_deref());
        let other_tag = other.node().and_then(|n| n.tag.as_deref());
        match (my_tag, other_tag) {
            (Some(a), Some(b)) => a.eq_ignore_ascii_case(b),
            _ => false,
        }
    }

    fn attr_matches(
        &self,
        ns: &NamespaceConstraint<&<Self::Impl as SelectorImpl>::NamespaceUrl>,
        local_name: &<Self::Impl as SelectorImpl>::LocalName,
        operation: &AttrSelectorOperation<&<Self::Impl as SelectorImpl>::AttrValue>,
    ) -> bool {
        // Namespace-filtrering: vi stödjer bara no-namespace
        match ns {
            NamespaceConstraint::Any => {}
            NamespaceConstraint::Specific(ns_url) => {
                if !ns_url.0.is_empty() {
                    return false;
                }
            }
        }

        let node = match self.node() {
            Some(n) => n,
            None => return false,
        };
        let attr_val = match node.get_attr(&local_name.0) {
            Some(v) => v,
            None => return false,
        };

        match operation {
            AttrSelectorOperation::Exists => true,
            AttrSelectorOperation::WithValue {
                operator,
                case_sensitivity,
                value,
            } => {
                let val = value.0.as_str();
                let actual = attr_val;
                match case_sensitivity {
                    CaseSensitivity::CaseSensitive => match_attr_value(operator, actual, val),
                    CaseSensitivity::AsciiCaseInsensitive => {
                        match_attr_value_ci(operator, actual, val)
                    }
                }
            }
        }
    }

    fn match_non_ts_pseudo_class(
        &self,
        pc: &ArenaPseudoClass,
        _context: &mut MatchingContext<Self::Impl>,
    ) -> bool {
        let node = match self.node() {
            Some(n) => n,
            None => return false,
        };
        match pc {
            ArenaPseudoClass::Checked => {
                let tag = node.tag.as_deref().unwrap_or("");
                if tag.eq_ignore_ascii_case("input") {
                    let input_type = node.get_attr("type").unwrap_or("text");
                    if input_type.eq_ignore_ascii_case("checkbox")
                        || input_type.eq_ignore_ascii_case("radio")
                    {
                        return node.attributes.contains_key("checked");
                    }
                }
                if tag.eq_ignore_ascii_case("option") {
                    return node.attributes.contains_key("selected");
                }
                false
            }
            ArenaPseudoClass::Disabled => node.attributes.contains_key("disabled"),
            ArenaPseudoClass::Enabled => !node.attributes.contains_key("disabled"),
            ArenaPseudoClass::Link | ArenaPseudoClass::AnyLink => {
                let tag = node.tag.as_deref().unwrap_or("");
                (tag.eq_ignore_ascii_case("a") || tag.eq_ignore_ascii_case("area"))
                    && node.attributes.contains_key("href")
            }
            ArenaPseudoClass::Required => node.attributes.contains_key("required"),
            ArenaPseudoClass::Optional => !node.attributes.contains_key("required"),
            ArenaPseudoClass::ReadOnly => node.attributes.contains_key("readonly"),
            ArenaPseudoClass::ReadWrite => !node.attributes.contains_key("readonly"),
            ArenaPseudoClass::Defined => true, // Alla kända element anses defined
            ArenaPseudoClass::Scope => {
                // :scope matchar scope-elementet (satt i MatchingContext)
                // Fallback: matchar root
                self.is_root()
            }
            // Tillstånds-pseudoklasser — statiskt DOM har ingen hover/focus/etc.
            ArenaPseudoClass::Hover
            | ArenaPseudoClass::Focus
            | ArenaPseudoClass::Active
            | ArenaPseudoClass::FocusVisible
            | ArenaPseudoClass::FocusWithin
            | ArenaPseudoClass::Visited
            | ArenaPseudoClass::Target
            | ArenaPseudoClass::Indeterminate
            | ArenaPseudoClass::PlaceholderShown
            | ArenaPseudoClass::Default
            | ArenaPseudoClass::Valid
            | ArenaPseudoClass::Invalid => false,
        }
    }

    fn match_pseudo_element(
        &self,
        _pe: &ArenaPseudoElement,
        _context: &mut MatchingContext<Self::Impl>,
    ) -> bool {
        false // Vi matchar inte pseudo-element i DOM-traversering
    }

    fn apply_selector_flags(&self, _flags: ElementSelectorFlags) {
        // Noop — vi behöver inte spåra selector flags
    }

    fn is_link(&self) -> bool {
        let node = match self.node() {
            Some(n) => n,
            None => return false,
        };
        let tag = node.tag.as_deref().unwrap_or("");
        (tag.eq_ignore_ascii_case("a") || tag.eq_ignore_ascii_case("area"))
            && node.attributes.contains_key("href")
    }

    fn is_html_slot_element(&self) -> bool {
        self.node()
            .and_then(|n| n.tag.as_deref())
            .is_some_and(|t| t.eq_ignore_ascii_case("slot"))
    }

    fn has_id(&self, id: &ArenaStr, case_sensitivity: CaseSensitivity) -> bool {
        self.node()
            .and_then(|n| n.get_attr("id"))
            .is_some_and(|actual_id| match case_sensitivity {
                CaseSensitivity::CaseSensitive => actual_id == id.0,
                CaseSensitivity::AsciiCaseInsensitive => actual_id.eq_ignore_ascii_case(&id.0),
            })
    }

    fn has_class(&self, name: &ArenaStr, case_sensitivity: CaseSensitivity) -> bool {
        self.node()
            .and_then(|n| n.get_attr("class"))
            .is_some_and(|class_str| {
                class_str
                    .split([' ', '\t', '\n', '\x0C', '\r'])
                    .filter(|s| !s.is_empty())
                    .any(|cls| match case_sensitivity {
                        CaseSensitivity::CaseSensitive => cls == name.0,
                        CaseSensitivity::AsciiCaseInsensitive => cls.eq_ignore_ascii_case(&name.0),
                    })
            })
    }

    fn has_custom_state(&self, _name: &ArenaStr) -> bool {
        false
    }

    fn imported_part(&self, _name: &ArenaStr) -> Option<ArenaStr> {
        None
    }

    fn is_part(&self, _name: &ArenaStr) -> bool {
        false
    }

    fn is_empty(&self) -> bool {
        let node = match self.node() {
            Some(n) => n,
            None => return true,
        };
        // :empty — inga element-barn eller textnoder med innehåll
        !node.children.iter().any(|&child_key| {
            self.arena
                .nodes
                .get(child_key)
                .is_some_and(|child| match child.node_type {
                    NodeType::Element => true,
                    NodeType::Text => child.text.as_ref().is_some_and(|t| !t.is_empty()),
                    _ => false,
                })
        })
    }

    fn is_root(&self) -> bool {
        // :root — matchar <html>-elementet (dokumentets rot-element)
        let node = match self.node() {
            Some(n) => n,
            None => return false,
        };
        if let Some(parent_key) = node.parent {
            if let Some(parent) = self.arena.nodes.get(parent_key) {
                return parent.node_type == NodeType::Document;
            }
        }
        false
    }

    fn add_element_unique_hashes(&self, _filter: &mut selectors::bloom::BloomFilter) -> bool {
        false
    }
}

// ─── Attribut-matchning hjälpfunktioner ──────────────────────────────────────

fn match_attr_value(op: &AttrSelectorOperator, actual: &str, expected: &str) -> bool {
    match op {
        AttrSelectorOperator::Equal => actual == expected,
        AttrSelectorOperator::Includes => actual
            .split([' ', '\t', '\n', '\x0C', '\r'])
            .any(|w| w == expected),
        AttrSelectorOperator::DashMatch => {
            actual == expected || actual.starts_with(&format!("{expected}-"))
        }
        AttrSelectorOperator::Prefix => !expected.is_empty() && actual.starts_with(expected),
        AttrSelectorOperator::Suffix => !expected.is_empty() && actual.ends_with(expected),
        AttrSelectorOperator::Substring => !expected.is_empty() && actual.contains(expected),
    }
}

fn match_attr_value_ci(op: &AttrSelectorOperator, actual: &str, expected: &str) -> bool {
    let actual_lower = actual.to_ascii_lowercase();
    let expected_lower = expected.to_ascii_lowercase();
    match_attr_value(op, &actual_lower, &expected_lower)
}

// ─── Publikt API: Parsa + matcha selektorer ──────────────────────────────────

/// Parsea en CSS-selektor till selectors-cratens AST.
/// Returnerar None vid parse-fel (ogiltiga selektorer).
fn parse_selector_list(selector: &str) -> Option<SelectorList<ArenaSelectorImpl>> {
    let mut input = cssparser::ParserInput::new(selector);
    let mut parser = cssparser::Parser::new(&mut input);
    SelectorList::parse(&ArenaParser, &mut parser, ParseRelative::No).ok()
}

/// Kontrollera om en nod matchar en CSS-selektor.
/// Använder selectors-cratens riktiga matching-engine.
pub(super) fn matches_selector(arena: &ArenaDom, key: NodeKey, selector: &str) -> bool {
    let selector = selector.trim();
    if selector.is_empty() {
        return false;
    }

    let selector_list = match parse_selector_list(selector) {
        Some(list) => list,
        None => return false,
    };

    let element = ArenaElement::new(arena, key);
    let mut caches = SelectorCaches::default();
    let mut context = MatchingContext::new(
        MatchingMode::Normal,
        None,
        &mut caches,
        QuirksMode::NoQuirks,
        NeedsSelectorFlags::No,
        MatchingForInvalidation::No,
    );

    selector_list.slice().iter().any(|selector| {
        selectors::matching::matches_selector(selector, 0, None, &element, &mut context)
    })
}

/// querySelector — hittar första matchande nod med full CSS-selektor
pub(super) fn query_select_one(arena: &ArenaDom, selector: &str) -> Option<NodeKey> {
    let selector = selector.trim();
    if selector.is_empty() {
        return None;
    }

    let selector_list = parse_selector_list(selector)?;

    let mut caches = SelectorCaches::default();
    let mut context = MatchingContext::new(
        MatchingMode::Normal,
        None,
        &mut caches,
        QuirksMode::NoQuirks,
        NeedsSelectorFlags::No,
        MatchingForInvalidation::No,
    );

    find_first_matching_compiled(arena, arena.document, &selector_list, &mut context)
}

fn find_first_matching_compiled(
    arena: &ArenaDom,
    key: NodeKey,
    selector_list: &SelectorList<ArenaSelectorImpl>,
    context: &mut MatchingContext<ArenaSelectorImpl>,
) -> Option<NodeKey> {
    let node = arena.nodes.get(key)?;
    if node.node_type == NodeType::Element {
        let element = ArenaElement::new(arena, key);
        if selector_list
            .slice()
            .iter()
            .any(|sel| selectors::matching::matches_selector(sel, 0, None, &element, context))
        {
            return Some(key);
        }
    }
    for &child in &node.children {
        if let Some(found) = find_first_matching_compiled(arena, child, selector_list, context) {
            return Some(found);
        }
    }
    None
}

/// querySelectorAll — returnerar alla matchande noder med full CSS-selektor
pub(super) fn query_select_all(arena: &ArenaDom, selector: &str) -> Vec<NodeKey> {
    let selector = selector.trim();
    if selector.is_empty() {
        return vec![];
    }

    let selector_list = match parse_selector_list(selector) {
        Some(list) => list,
        None => return vec![],
    };

    let mut results = vec![];
    let mut caches = SelectorCaches::default();
    let mut context = MatchingContext::new(
        MatchingMode::Normal,
        None,
        &mut caches,
        QuirksMode::NoQuirks,
        NeedsSelectorFlags::No,
        MatchingForInvalidation::No,
    );

    find_all_matching_compiled(
        arena,
        arena.document,
        &selector_list,
        &mut context,
        &mut results,
    );
    results
}

fn find_all_matching_compiled(
    arena: &ArenaDom,
    key: NodeKey,
    selector_list: &SelectorList<ArenaSelectorImpl>,
    context: &mut MatchingContext<ArenaSelectorImpl>,
    results: &mut Vec<NodeKey>,
) {
    let node = match arena.nodes.get(key) {
        Some(n) => n,
        None => return,
    };
    if node.node_type == NodeType::Element {
        let element = ArenaElement::new(arena, key);
        if selector_list
            .slice()
            .iter()
            .any(|sel| selectors::matching::matches_selector(sel, 0, None, &element, context))
        {
            results.push(key);
        }
    }
    let children: Vec<NodeKey> = node.children.clone();
    for child in children {
        find_all_matching_compiled(arena, child, selector_list, context, results);
    }
}

/// Bakåtkompatibel wrapper: hittar första matchande nod under given nod
pub(super) fn find_first_matching(
    arena: &ArenaDom,
    key: NodeKey,
    selector: &str,
) -> Option<NodeKey> {
    let selector = selector.trim();
    if selector.is_empty() {
        return None;
    }
    let selector_list = parse_selector_list(selector)?;
    let mut caches = SelectorCaches::default();
    let mut context = MatchingContext::new(
        MatchingMode::Normal,
        None,
        &mut caches,
        QuirksMode::NoQuirks,
        NeedsSelectorFlags::No,
        MatchingForInvalidation::No,
    );
    // Per spec: element.querySelector() söker bara bland descendants
    let node = arena.nodes.get(key)?;
    for &child in &node.children {
        if let Some(found) =
            find_first_matching_compiled(arena, child, &selector_list, &mut context)
        {
            return Some(found);
        }
    }
    None
}

/// Bakåtkompatibel wrapper: hittar alla matchande noder under given nod
pub(super) fn find_all_matching(
    arena: &ArenaDom,
    key: NodeKey,
    selector: &str,
    results: &mut Vec<NodeKey>,
) {
    let selector = selector.trim();
    if selector.is_empty() {
        return;
    }
    let selector_list = match parse_selector_list(selector) {
        Some(list) => list,
        None => return,
    };
    let mut caches = SelectorCaches::default();
    let mut context = MatchingContext::new(
        MatchingMode::Normal,
        None,
        &mut caches,
        QuirksMode::NoQuirks,
        NeedsSelectorFlags::No,
        MatchingForInvalidation::No,
    );
    // Per spec: element.querySelectorAll() söker bara bland descendants, inte elementet själv.
    // Starta från barn istället för key.
    if let Some(node) = arena.nodes.get(key) {
        let children: Vec<NodeKey> = node.children.clone();
        for child in children {
            find_all_matching_compiled(arena, child, &selector_list, &mut context, results);
        }
    }
}

// ─── Hjälpfunktioner som används av andra moduler ────────────────────────────

/// Splitta sträng på ASCII whitespace per HTML-spec
fn split_ascii_whitespace(s: &str) -> impl Iterator<Item = &str> {
    s.split([' ', '\t', '\n', '\x0C', '\r'])
        .filter(|s| !s.is_empty())
}

/// Samla alla element med given klass
pub(super) fn find_all_by_class(
    arena: &ArenaDom,
    key: NodeKey,
    class: &str,
    results: &mut Vec<NodeKey>,
) {
    let node = match arena.nodes.get(key) {
        Some(n) => n,
        None => return,
    };
    if node.node_type == NodeType::Element {
        if let Some(attr_classes) = node.get_attr("class") {
            let search_tokens: Vec<&str> = split_ascii_whitespace(class).collect();
            if !search_tokens.is_empty() {
                let elem_tokens: Vec<&str> = split_ascii_whitespace(attr_classes).collect();
                if search_tokens.iter().all(|t| elem_tokens.contains(t)) {
                    results.push(key);
                }
            }
        }
    }
    let children: Vec<NodeKey> = node.children.clone();
    for child in children {
        find_all_by_class(arena, child, class, results);
    }
}

/// Samla alla element med given tagg bland ättlingar (exkluderar root).
/// I HTML-dokument matchar getElementsByTagName bara element i HTML namespace.
pub(super) fn find_all_by_tag(
    arena: &ArenaDom,
    key: NodeKey,
    tag: &str,
    results: &mut Vec<NodeKey>,
) {
    let node = match arena.nodes.get(key) {
        Some(n) => n,
        None => return,
    };
    // Starta från barnens barn (root exkluderas per spec)
    let children: Vec<NodeKey> = node.children.clone();
    for child in children {
        find_all_by_tag_recursive(arena, child, tag, results);
    }
}

fn find_all_by_tag_recursive(
    arena: &ArenaDom,
    key: NodeKey,
    tag: &str,
    results: &mut Vec<NodeKey>,
) {
    let node = match arena.nodes.get(key) {
        Some(n) => n,
        None => return,
    };
    if node.node_type == NodeType::Element {
        if tag == "*" {
            // Wildcard matchar alla element oavsett namespace
            results.push(key);
        } else {
            let node_ns = get_node_namespace(node);
            let is_html_ns = node_ns == "http://www.w3.org/1999/xhtml";
            if is_html_ns {
                // HTML namespace: input ASCII-lowercasas, jämför exakt mot elementets qualifiedName
                let tag_lower = tag.to_ascii_lowercase();
                if node.tag.as_deref().is_some_and(|t| t == tag_lower) {
                    results.push(key);
                }
            } else {
                // Icke-HTML namespace: case-sensitive exakt match mot original input
                if node.tag.as_deref().is_some_and(|t| t == tag) {
                    results.push(key);
                }
            }
        }
    }
    let children: Vec<NodeKey> = node.children.clone();
    for child in children {
        find_all_by_tag_recursive(arena, child, tag, results);
    }
}

/// Hämta effektiv namespace-URI för en nod.
/// Element utan __ns__-attribut antas tillhöra XHTML-namnrymden (HTML-parsade element).
fn get_node_namespace(node: &crate::arena_dom::DomNode) -> &str {
    const XHTML_NS: &str = "http://www.w3.org/1999/xhtml";
    match node.attributes.get("__ns__") {
        Some(ns) => {
            if ns.is_empty() {
                "" // null namespace
            } else {
                ns.as_str()
            }
        }
        None => XHTML_NS, // HTML-parsade element
    }
}

/// Samla alla element som matchar namespace + localName (case-sensitive).
/// ns="*" matchar alla namespaces, local_name="*" matchar alla localNames.
/// Söker bland alla ättlingar till root-noden (exkluderar root-noden själv).
pub(super) fn find_all_by_tag_ns(
    arena: &ArenaDom,
    key: NodeKey,
    ns: &str,
    local_name: &str,
    results: &mut Vec<NodeKey>,
) {
    let node = match arena.nodes.get(key) {
        Some(n) => n,
        None => return,
    };
    let children: Vec<NodeKey> = node.children.clone();
    for child in children {
        find_all_by_tag_ns_recursive(arena, child, ns, local_name, results);
    }
}

fn find_all_by_tag_ns_recursive(
    arena: &ArenaDom,
    key: NodeKey,
    ns: &str,
    local_name: &str,
    results: &mut Vec<NodeKey>,
) {
    let node = match arena.nodes.get(key) {
        Some(n) => n,
        None => return,
    };
    if node.node_type == NodeType::Element {
        let node_ns = get_node_namespace(node);
        let ns_match = ns == "*" || node_ns == ns;
        let local_match = local_name == "*"
            || node.tag.as_deref().is_some_and(|t| {
                // Jämför mot localName (efter : om prefix finns)
                let node_local = if let Some(colon) = t.find(':') {
                    &t[colon + 1..]
                } else {
                    t
                };
                node_local == local_name
            });
        if ns_match && local_match {
            results.push(key);
        }
    }
    let children: Vec<NodeKey> = node.children.clone();
    for child in children {
        find_all_by_tag_ns_recursive(arena, child, ns, local_name, results);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::arena_dom_sink::parse_html_to_arena;

    #[test]
    fn test_has_selector_basic() {
        let html =
            r#"<div id="parent"><span class="child">Hello</span></div><div id="other">Bye</div>"#;
        let arena = parse_html_to_arena(html);

        // :has(.child) borde matcha #parent (och html/body)
        let results = query_select_all(&arena, ":has(.child)");
        let ids: Vec<&str> = results
            .iter()
            .filter_map(|k| arena.nodes.get(*k))
            .filter_map(|n| n.get_attr("id"))
            .collect();
        println!(":has(.child) matched ids: {:?}", ids);
        assert!(
            ids.contains(&"parent"),
            ":has(.child) borde matcha parent, fick: {:?}",
            ids
        );
    }

    #[test]
    fn test_has_child_combinator() {
        let html = r#"<div id="a"><span class="x">Hi</span></div><div id="b"><p><span class="x">Deep</span></p></div>"#;
        let arena = parse_html_to_arena(html);

        // :has(> .x) borde matcha bara #a (direkt barn), inte #b (nested)
        let results = query_select_all(&arena, ":has(> .x)");
        let ids: Vec<&str> = results
            .iter()
            .filter_map(|k| arena.nodes.get(*k))
            .filter_map(|n| n.get_attr("id"))
            .collect();
        println!(":has(> .x) matched ids: {:?}", ids);
        assert!(
            ids.contains(&"a"),
            ":has(> .x) borde matcha #a, fick: {:?}",
            ids
        );
        assert!(
            !ids.contains(&"b"),
            ":has(> .x) borde INTE matcha #b (nested), fick: {:?}",
            ids
        );
    }

    #[test]
    fn test_has_sibling_combinator() {
        let html = r#"<div id="wrap"><span id="a" class="first"></span><span id="b" class="second"></span></div>"#;
        let arena = parse_html_to_arena(html);

        // :has(+ .second) borde matcha #a (adjacent sibling)
        let results = query_select_all(&arena, ":has(+ .second)");
        let ids: Vec<&str> = results
            .iter()
            .filter_map(|k| arena.nodes.get(*k))
            .filter_map(|n| n.get_attr("id"))
            .collect();
        println!(":has(+ .second) matched ids: {:?}", ids);
        assert!(
            ids.contains(&"a"),
            ":has(+ .second) borde matcha #a, fick: {:?}",
            ids
        );
    }

    #[test]
    fn test_basic_class_selector() {
        let html = r#"<div class="foo">A</div><div class="bar">B</div>"#;
        let arena = parse_html_to_arena(html);

        let results = query_select_all(&arena, ".foo");
        assert_eq!(results.len(), 1, ".foo borde matcha 1 element");
    }

    #[test]
    fn test_is_and_where_selector() {
        let html = r#"<div class="a">A</div><span class="b">B</span><p class="c">C</p>"#;
        let arena = parse_html_to_arena(html);

        let results = query_select_all(&arena, ":is(.a, .b)");
        assert_eq!(
            results.len(),
            2,
            ":is(.a, .b) borde matcha 2 element, fick: {}",
            results.len()
        );
    }
}
