// CSSOM — CSS Object Model
// https://drafts.csswg.org/cssom/
// Interfaces för css/cssom WPT-sviten

interface CSSStyleDeclaration {
  attribute DOMString cssText;
  readonly attribute unsigned long length;
  DOMString item(unsigned long index);
  DOMString getPropertyValue(DOMString property);
  DOMString getPropertyPriority(DOMString property);
  void setProperty(DOMString property, DOMString value, optional DOMString priority);
  DOMString removeProperty(DOMString property);
  readonly attribute CSSRule parentRule;
  attribute DOMString cssFloat;
};

interface CSSRule {
  readonly attribute unsigned short type;
  attribute DOMString cssText;
  readonly attribute CSSRule parentRule;
  readonly attribute CSSStyleSheet parentStyleSheet;
};

interface CSSStyleRule : CSSRule {
  attribute DOMString selectorText;
  readonly attribute CSSStyleDeclaration style;
};

interface CSSMediaRule : CSSRule {
  readonly attribute MediaList media;
  readonly attribute CSSRuleList cssRules;
  unsigned long insertRule(DOMString rule, optional unsigned long index);
  void deleteRule(unsigned long index);
};

interface CSSImportRule : CSSRule {
  readonly attribute DOMString href;
  readonly attribute MediaList media;
  readonly attribute CSSStyleSheet styleSheet;
};

interface StyleSheet {
  readonly attribute DOMString type;
  readonly attribute DOMString href;
  readonly attribute Node ownerNode;
  readonly attribute StyleSheet parentStyleSheet;
  readonly attribute DOMString title;
  readonly attribute MediaList media;
  attribute boolean disabled;
};

interface CSSStyleSheet : StyleSheet {
  readonly attribute CSSRule ownerRule;
  readonly attribute CSSRuleList cssRules;
  unsigned long insertRule(DOMString rule, optional unsigned long index);
  void deleteRule(unsigned long index);
  void replace(DOMString text);
  void replaceSync(DOMString text);
};

interface MediaList {
  attribute DOMString mediaText;
  readonly attribute unsigned long length;
  DOMString item(unsigned long index);
  void appendMedium(DOMString medium);
  void deleteMedium(DOMString medium);
};

interface CSSRuleList {
  readonly attribute unsigned long length;
  CSSRule item(unsigned long index);
};

interface StyleSheetList {
  readonly attribute unsigned long length;
  StyleSheet item(unsigned long index);
};
