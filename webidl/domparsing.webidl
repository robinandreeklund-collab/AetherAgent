// DOM Parsing and Serialization
// https://html.spec.whatwg.org/multipage/dynamic-markup-insertion.html
// https://w3c.github.io/DOM-Parsing/
// Interfaces för domparsing WPT-sviten

interface DOMParser {
  Document parseFromString(DOMString string, DOMString type);
};

interface XMLSerializer {
  DOMString serializeToString(Node root);
};

// InnerHTML mixin (appliceras på Element och ShadowRoot)
interface InnerHTML {
  attribute DOMString innerHTML;
};

// Range tillägg för domparsing
interface RangeFragment {
  DocumentFragment createContextualFragment(DOMString fragment);
};
