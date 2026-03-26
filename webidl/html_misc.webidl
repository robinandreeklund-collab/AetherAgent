// Diverse HTML element interfaces — from HTML Living Standard
// Behövs för html/semantics och domparsing tester

interface HTMLDivElement : HTMLElement {
};

interface HTMLSpanElement : HTMLElement {
};

interface HTMLParagraphElement : HTMLElement {
};

interface HTMLHeadingElement : HTMLElement {
};

interface HTMLPreElement : HTMLElement {
};

interface HTMLQuoteElement : HTMLElement {
  attribute DOMString cite;
};

interface HTMLOListElement : HTMLElement {
  attribute boolean reversed;
  attribute long start;
  attribute DOMString type;
};

interface HTMLUListElement : HTMLElement {
};

interface HTMLLIElement : HTMLElement {
  attribute long value;
};

interface HTMLDListElement : HTMLElement {
};

interface HTMLHRElement : HTMLElement {
};

interface HTMLBRElement : HTMLElement {
};

interface HTMLTableElement : HTMLElement {
  attribute DOMString border;
  attribute DOMString frame;
  attribute DOMString rules;
  attribute DOMString summary;
  attribute DOMString width;
};

interface HTMLTableSectionElement : HTMLElement {
};

interface HTMLTableRowElement : HTMLElement {
  readonly attribute long rowIndex;
  readonly attribute long sectionRowIndex;
};

interface HTMLTableCellElement : HTMLElement {
  attribute unsigned long colSpan;
  attribute unsigned long rowSpan;
  attribute DOMString headers;
  readonly attribute long cellIndex;
  attribute DOMString scope;
  attribute DOMString abbr;
};

interface HTMLTableCaptionElement : HTMLElement {
};

interface HTMLTableColElement : HTMLElement {
  attribute unsigned long span;
};

interface HTMLIFrameElement : HTMLElement {
  attribute DOMString src;
  attribute DOMString srcdoc;
  attribute DOMString name;
  attribute DOMString sandbox;
  attribute boolean allowFullscreen;
  attribute DOMString allow;
  attribute DOMString width;
  attribute DOMString height;
  attribute DOMString referrerPolicy;
  attribute DOMString loading;
};

interface HTMLEmbedElement : HTMLElement {
  attribute DOMString src;
  attribute DOMString type;
  attribute DOMString width;
  attribute DOMString height;
};

interface HTMLObjectElement : HTMLElement {
  attribute DOMString data;
  attribute DOMString type;
  attribute DOMString name;
  attribute DOMString width;
  attribute DOMString height;
};

interface HTMLDialogElement : HTMLElement {
  attribute boolean open;
  attribute DOMString returnValue;
  void show();
  void showModal();
  void close(optional DOMString returnValue);
};

interface HTMLDetailsElement : HTMLElement {
  attribute boolean open;
  attribute DOMString name;
};

interface HTMLSummaryElement : HTMLElement {
};

interface HTMLTemplateElement : HTMLElement {
};

interface HTMLSlotElement : HTMLElement {
  attribute DOMString name;
};

interface HTMLCanvasElement : HTMLElement {
  attribute unsigned long width;
  attribute unsigned long height;
};

interface HTMLScriptElement : HTMLElement {
  attribute DOMString src;
  attribute DOMString type;
  attribute boolean noModule;
  attribute boolean async;
  attribute boolean defer;
  attribute DOMString crossOrigin;
  attribute DOMString text;
  attribute DOMString integrity;
  attribute DOMString referrerPolicy;
};

interface HTMLStyleElement : HTMLElement {
  attribute boolean disabled;
  attribute DOMString media;
  attribute DOMString type;
};

interface HTMLLinkElement : HTMLElement {
  attribute DOMString href;
  attribute DOMString crossOrigin;
  attribute DOMString rel;
  attribute DOMString as;
  attribute DOMString media;
  attribute DOMString integrity;
  attribute DOMString hreflang;
  attribute DOMString type;
  attribute DOMString referrerPolicy;
  attribute boolean disabled;
};

interface HTMLMetaElement : HTMLElement {
  attribute DOMString name;
  attribute DOMString httpEquiv;
  attribute DOMString content;
  attribute DOMString media;
};

interface HTMLBaseElement : HTMLElement {
  attribute DOMString href;
  attribute DOMString target;
};

interface HTMLTitleElement : HTMLElement {
  attribute DOMString text;
};

interface HTMLBodyElement : HTMLElement {
  attribute DOMString text;
  attribute DOMString link;
  attribute DOMString vLink;
  attribute DOMString aLink;
  attribute DOMString bgColor;
  attribute DOMString background;
};

interface HTMLHtmlElement : HTMLElement {
  attribute DOMString version;
};

interface HTMLHeadElement : HTMLElement {
};

interface HTMLAreaElement : HTMLElement {
  attribute DOMString alt;
  attribute DOMString coords;
  attribute DOMString download;
  attribute DOMString href;
  attribute DOMString hreflang;
  attribute DOMString ping;
  attribute DOMString referrerPolicy;
  attribute DOMString rel;
  attribute DOMString shape;
  attribute DOMString target;
};

interface HTMLMapElement : HTMLElement {
  attribute DOMString name;
};

interface HTMLDataElement : HTMLElement {
  attribute DOMString value;
};

interface HTMLTimeElement : HTMLElement {
  attribute DOMString dateTime;
};

interface HTMLPictureElement : HTMLElement {
};

interface HTMLOptGroupElement : HTMLElement {
  attribute boolean disabled;
  attribute DOMString label;
};

interface HTMLDataListElement : HTMLElement {
};

interface HTMLMenuElement : HTMLElement {
};
