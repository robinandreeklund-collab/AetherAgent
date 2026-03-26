// HTMLInputElement — from HTML Living Standard
// https://html.spec.whatwg.org/multipage/input.html#htmlinputelement
// Simplified for AetherAgent PoC — attribute reflection only

interface HTMLInputElement : HTMLElement {
  attribute DOMString accept;
  attribute DOMString alt;
  attribute DOMString autocomplete;
  attribute boolean defaultChecked;
  attribute boolean checked;
  attribute DOMString dirName;
  attribute boolean disabled;
  attribute DOMString formAction;
  attribute DOMString formEnctype;
  attribute DOMString formMethod;
  attribute boolean formNoValidate;
  attribute DOMString formTarget;
  attribute unsigned long height;
  attribute boolean indeterminate;
  attribute DOMString max;
  attribute long maxLength;
  attribute DOMString min;
  attribute long minLength;
  attribute boolean multiple;
  attribute DOMString name;
  attribute DOMString pattern;
  attribute DOMString placeholder;
  attribute boolean readOnly;
  attribute boolean required;
  attribute unsigned long size;
  attribute DOMString src;
  attribute DOMString step;
  attribute DOMString type;
  attribute DOMString defaultValue;
  attribute DOMString value;
  attribute unsigned long width;

  void select();
  void setCustomValidity(DOMString error);
  boolean checkValidity();
  boolean reportValidity();

  readonly attribute DOMString validationMessage;
  readonly attribute boolean willValidate;
  readonly attribute NodeList labels;
};

interface HTMLButtonElement : HTMLElement {
  attribute boolean disabled;
  attribute DOMString formAction;
  attribute DOMString formEnctype;
  attribute DOMString formMethod;
  attribute boolean formNoValidate;
  attribute DOMString formTarget;
  attribute DOMString name;
  attribute DOMString type;
  attribute DOMString value;

  readonly attribute boolean willValidate;
  readonly attribute DOMString validationMessage;
  boolean checkValidity();
  boolean reportValidity();
  void setCustomValidity(DOMString error);
  readonly attribute NodeList labels;
};

interface HTMLSelectElement : HTMLElement {
  attribute DOMString autocomplete;
  attribute boolean disabled;
  attribute long length;
  attribute boolean multiple;
  attribute DOMString name;
  attribute boolean required;
  attribute long selectedIndex;
  attribute unsigned long size;
  attribute DOMString value;

  readonly attribute DOMString type;
  readonly attribute boolean willValidate;
  readonly attribute DOMString validationMessage;
  boolean checkValidity();
  boolean reportValidity();
  void setCustomValidity(DOMString error);
  readonly attribute NodeList labels;
};

interface HTMLTextAreaElement : HTMLElement {
  attribute DOMString autocomplete;
  attribute long cols;
  attribute DOMString defaultValue;
  attribute DOMString dirName;
  attribute boolean disabled;
  attribute long maxLength;
  attribute long minLength;
  attribute DOMString name;
  attribute DOMString placeholder;
  attribute boolean readOnly;
  attribute boolean required;
  attribute long rows;
  attribute DOMString value;
  attribute DOMString wrap;

  readonly attribute DOMString type;
  readonly attribute unsigned long textLength;
  readonly attribute boolean willValidate;
  readonly attribute DOMString validationMessage;
  boolean checkValidity();
  boolean reportValidity();
  void setCustomValidity(DOMString error);
  void select();
  readonly attribute NodeList labels;
};

interface HTMLFormElement : HTMLElement {
  attribute DOMString acceptCharset;
  attribute DOMString action;
  attribute DOMString autocomplete;
  attribute DOMString encoding;
  attribute DOMString enctype;
  attribute DOMString method;
  attribute DOMString name;
  attribute boolean noValidate;
  attribute DOMString target;

  readonly attribute long length;
  void submit();
  void requestSubmit();
  void reset();
  boolean checkValidity();
  boolean reportValidity();
};

interface HTMLAnchorElement : HTMLElement {
  attribute DOMString target;
  attribute DOMString download;
  attribute DOMString ping;
  attribute DOMString rel;
  attribute DOMString hreflang;
  attribute DOMString type;
  attribute DOMString text;
  attribute DOMString referrerPolicy;

  attribute DOMString href;
  readonly attribute DOMString origin;
  attribute DOMString protocol;
  attribute DOMString username;
  attribute DOMString password;
  attribute DOMString host;
  attribute DOMString hostname;
  attribute DOMString port;
  attribute DOMString pathname;
  attribute DOMString search;
  attribute DOMString hash;
};

interface HTMLImageElement : HTMLElement {
  attribute DOMString alt;
  attribute DOMString src;
  attribute DOMString srcset;
  attribute DOMString sizes;
  attribute DOMString crossOrigin;
  attribute DOMString useMap;
  attribute boolean isMap;
  attribute unsigned long width;
  attribute unsigned long height;
  attribute DOMString decoding;
  attribute DOMString loading;
  attribute DOMString referrerPolicy;

  readonly attribute unsigned long naturalWidth;
  readonly attribute unsigned long naturalHeight;
  readonly attribute boolean complete;
  readonly attribute DOMString currentSrc;
};

interface HTMLOptionElement : HTMLElement {
  attribute boolean disabled;
  attribute DOMString label;
  attribute boolean defaultSelected;
  attribute boolean selected;
  attribute DOMString value;
  attribute DOMString text;
  readonly attribute long index;
};

interface HTMLLabelElement : HTMLElement {
  attribute DOMString htmlFor;
};

interface HTMLFieldSetElement : HTMLElement {
  attribute boolean disabled;
  attribute DOMString name;
  readonly attribute DOMString type;
  boolean checkValidity();
  boolean reportValidity();
  void setCustomValidity(DOMString error);
};

interface HTMLOutputElement : HTMLElement {
  attribute DOMString defaultValue;
  attribute DOMString name;
  readonly attribute DOMString type;
  attribute DOMString value;
  readonly attribute boolean willValidate;
  readonly attribute DOMString validationMessage;
  boolean checkValidity();
  boolean reportValidity();
  void setCustomValidity(DOMString error);
};

interface HTMLLegendElement : HTMLElement {
};

interface HTMLProgressElement : HTMLElement {
  attribute double value;
  attribute double max;
  readonly attribute double position;
  readonly attribute NodeList labels;
};

interface HTMLMeterElement : HTMLElement {
  attribute double value;
  attribute double min;
  attribute double max;
  attribute double low;
  attribute double high;
  attribute double optimum;
  readonly attribute NodeList labels;
};
