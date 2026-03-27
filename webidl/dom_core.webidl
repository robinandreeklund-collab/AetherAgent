// DOM Core Interfaces
// https://dom.spec.whatwg.org/
// Interfaces för dom/lists, dom/collections WPT-sviterna

interface DOMTokenList {
  readonly attribute unsigned long length;
  DOMString item(unsigned long index);
  boolean contains(DOMString token);
  void add(DOMString... tokens);
  void remove(DOMString... tokens);
  boolean toggle(DOMString token, optional boolean force);
  boolean replace(DOMString token, DOMString newToken);
  boolean supports(DOMString token);
  attribute DOMString value;
};

interface NamedNodeMap {
  readonly attribute unsigned long length;
  Attr item(unsigned long index);
  Attr getNamedItem(DOMString qualifiedName);
  Attr getNamedItemNS(DOMString namespace, DOMString localName);
  Attr setNamedItem(Attr attr);
  Attr setNamedItemNS(Attr attr);
  Attr removeNamedItem(DOMString qualifiedName);
  Attr removeNamedItemNS(DOMString namespace, DOMString localName);
};

interface HTMLCollection {
  readonly attribute unsigned long length;
  Element item(unsigned long index);
  Element namedItem(DOMString name);
};

interface NodeList {
  readonly attribute unsigned long length;
  Node item(unsigned long index);
};

interface HTMLFormControlsCollection : HTMLCollection {
  Element namedItem(DOMString name);
};

interface HTMLOptionsCollection : HTMLCollection {
  attribute unsigned long length;
  attribute long selectedIndex;
  void add(HTMLOptionElement element, optional long before);
  void remove(long index);
};
