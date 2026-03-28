/*
 * AetherAgent WPT Polyfills
 *
 * Fyller luckor i DOM bridge som WPT-tester förväntar sig.
 * Laddas före testharness.js.
 */

// ─── CharacterData: native Rust (dom_bridge/mod.rs) ──────────────────────────
// .data, .nodeValue, .length = Rust getter/setter i make_element_object()
// substringData, appendData, etc. = Rust-native i chardata.rs
globalThis.__patchCharacterData = function(n) { return n; };

// ─── document.implementation ─────────────────────────────────────────────────
// DOMImplementation med createDocument, createHTMLDocument, createDocumentType
(function() {
  if (typeof document === 'undefined') return;
  if (!document.implementation) {
    document.implementation = {};
  }
  var impl = document.implementation;

  if (!impl.createHTMLDocument) {
    impl.createHTMLDocument = function(title) {
      // Bygg en riktig DOM-struktur via vår arena
      var html = document.createElement('html');
      var head = document.createElement('head');
      var body = document.createElement('body');
      html.appendChild(head);
      html.appendChild(body);
      if (title !== undefined) {
        var titleEl = document.createElement('title');
        var titleText = (title === null) ? 'null' : String(title);
        titleEl.appendChild(document.createTextNode(titleText));
        head.appendChild(titleEl);
      }
      // Skapa ett dokument-liknande objekt via DocumentFragment (behåller arena-koppling)
      var doc = document.createDocumentFragment();
      doc.appendChild(html);
      // Lägg till document-liknande egenskaper
      doc.nodeType = 9;
      doc.nodeName = '#document';
      doc.nodeValue = null;
      doc.documentElement = html;
      // Skapa doctype-nod (lazy — sätts efter doc är klar)
      try {
        if (document.__createDocumentType) {
          var dt = document.__createDocumentType('html', '', '');
          doc.doctype = dt;
        }
      } catch(e) {
        doc.doctype = null;
      }
      doc.head = head;
      doc.body = body;
      doc.title = (title === null) ? 'null' : (title || '');
      // Metadata per spec
      doc.URL = 'about:blank';
      doc.documentURI = 'about:blank';
      doc.compatMode = 'CSS1Compat';
      doc.characterSet = 'UTF-8';
      doc.charset = 'UTF-8';
      doc.inputEncoding = 'UTF-8';
      doc.contentType = 'text/html';
      doc.location = null;
      // Per-doc implementation med ownerDoc-referens
      var docImpl = Object.create(document.implementation);
      docImpl._ownerDoc = doc;
      doc.implementation = docImpl;
      doc.createElement = document.createElement.bind(document);
      doc.createTextNode = document.createTextNode.bind(document);
      doc.createComment = document.createComment.bind(document);
      doc.createDocumentFragment = document.createDocumentFragment.bind(document);
      doc.createElementNS = document.createElementNS ? document.createElementNS.bind(document) : undefined;
      // Query-metoder söker i detta dokumentets träd
      doc.getElementById = function(id) {
        // Rekursiv sökning i hela dokumentträdet
        function findById(node, target) {
          if (node.id === target) return node;
          var kids = node.childNodes || [];
          for (var i = 0; i < kids.length; i++) {
            var found = findById(kids[i], target);
            if (found) return found;
          }
          return null;
        }
        return findById(html, id);
      };
      doc.querySelector = function(sel) { return html.querySelector(sel); };
      doc.querySelectorAll = function(sel) { return html.querySelectorAll(sel); };
      doc.getElementsByTagName = function(tag) { return html.getElementsByTagName(tag); };
      doc.getElementsByClassName = function(cls) { return html.getElementsByClassName(cls); };
      doc.getElementsByTagNameNS = function(ns, tag) { return html.getElementsByTagNameNS ? html.getElementsByTagNameNS(ns, tag) : []; };
      doc.adoptNode = function(node) { return node; };
      doc.importNode = function(node, deep) { return node.cloneNode(deep); };
      doc.createRange = function() {
        if (typeof Range !== 'undefined') return new Range();
        return document.createRange();
      };
      doc.createTreeWalker = function(root, show, filter) {
        return document.createTreeWalker(root, show, filter);
      };
      doc.createNodeIterator = function(root, show, filter) {
        return document.createNodeIterator(root, show, filter);
      };
      doc.createCDATASection = function(data) {
        var node = document.createComment(data);
        node.nodeType = 4; // CDATA_SECTION_NODE
        node.nodeName = '#cdata-section';
        return node;
      };
      doc.createProcessingInstruction = function(target, data) {
        // Delegera till native om möjlig
        if (document.createProcessingInstruction) {
          return document.createProcessingInstruction(target, data);
        }
        var node = document.createComment(data);
        node.nodeType = 7;
        node.nodeName = target;
        node.target = target;
        node.data = data;
        return node;
      };
      // Event delegation
      doc.addEventListener = function() {};
      doc.removeEventListener = function() {};
      doc.dispatchEvent = function() { return true; };
      return doc;
    };
  }

  if (!impl.createDocument) {
    impl.createDocument = function(namespace, qualifiedName, doctype) {
      var doc = impl.createHTMLDocument('');
      doc.contentType = namespace ? 'application/xml' : 'text/html';
      return doc;
    };
  }

  if (!impl.createDocumentType) {
    impl.createDocumentType = function(qualifiedName, publicId, systemId) {
      var ownerDoc = this._ownerDoc || document;
      // Använd native arena-nod
      if (ownerDoc === document && document.__createDocumentType) {
        return document.__createDocumentType(qualifiedName || '', publicId || '', systemId || '');
      }
      var dt = document.__createDocumentType
        ? document.__createDocumentType(qualifiedName || '', publicId || '', systemId || '')
        : null;
      if (dt) {
        // Override ownerDocument getter om det är en foreign doc
        if (ownerDoc !== document) {
          try {
            Object.defineProperty(dt, 'ownerDocument', { get: function() { return ownerDoc; }, configurable: true });
          } catch(e) {}
        }
        return dt;
      }
      return {
        nodeType: 10,
        nodeName: qualifiedName || '',
        name: qualifiedName || '',
        publicId: publicId || '',
        systemId: systemId || '',
        ownerDocument: ownerDoc
      };
    };
  }

  if (!impl.hasFeature) {
    impl.hasFeature = function() { return true; };
  }
})();

// ─── document.title — MIGRERAD till native Rust (DocTitleGetter/Setter) ──────

// ─── document.URL / document.location — native Rust (register_document) ──────
// URL sätts till "about:blank" i Rust. location alias sätts i register_window.

// ─── Event-typ-konstruktorer ─────────────────────────────────────────────────
// MIGRERAD: UIEvent, MouseEvent, KeyboardEvent, FocusEvent, InputEvent,
// WheelEvent, PointerEvent, CompositionEvent → native i dom_bridge.rs
// Kvar: enklare event-typer som inte ännu migrerats
(function() {
  if (typeof Event === 'undefined') return;
  // Enkla event-typer (ärver Event direkt, inga spec-properties)
  var simpleTypes = [
    'TouchEvent', 'AnimationEvent', 'TransitionEvent',
    'HashChangeEvent', 'PopStateEvent', 'StorageEvent', 'PageTransitionEvent',
    'ProgressEvent', 'ClipboardEvent', 'DragEvent', 'ErrorEvent',
    'MessageEvent', 'PromiseRejectionEvent', 'SecurityPolicyViolationEvent',
    'DeviceMotionEvent', 'DeviceOrientationEvent', 'GamepadEvent',
    'MediaQueryListEvent', 'FormDataEvent', 'SubmitEvent', 'BeforeUnloadEvent'
  ];
  simpleTypes.forEach(function(name) {
    if (!globalThis[name]) {
      globalThis[name] = function(type, opts) { Event.call(this, type, opts); };
      globalThis[name].prototype = Object.create(Event.prototype);
      globalThis[name].prototype.constructor = globalThis[name];
    }
  });
})();

// ─── document.createEvent() — MIGRERAD till native Rust (dom_bridge/mod.rs) ──
// Registrerad som NativeCreateEvent i register_document().

// ─── node.ownerDocument ─────────────────────────────────────────────────────
// WPT-tester kontrollerar ofta att noder hör till rätt dokument
(function() {
  if (typeof document === 'undefined') return;

  // Patcha createElement så att returnerade element har ownerDocument
  var _origCreateElement = document.createElement;
  if (_origCreateElement) {
    document.createElement = function(tag) {
      var el = _origCreateElement.call(document, tag);
      if (el && !('ownerDocument' in el)) {
        try {
          Object.defineProperty(el, 'ownerDocument', {
            get: function() { return document; },
            configurable: true
          });
        } catch(e) {
          el.ownerDocument = document;
        }
      }
      return el;
    };
  }

  // Patcha createTextNode
  var _origCreateTextNode = document.createTextNode;
  if (_origCreateTextNode) {
    document.createTextNode = function(text) {
      var node = _origCreateTextNode.call(document, text);
      if (node && !('ownerDocument' in node)) {
        try {
          Object.defineProperty(node, 'ownerDocument', {
            get: function() { return document; },
            configurable: true
          });
        } catch(e) {
          node.ownerDocument = document;
        }
      }
      return node;
    };
  }

  // Patcha createComment
  var _origCreateComment = document.createComment;
  if (_origCreateComment) {
    document.createComment = function(text) {
      var node = _origCreateComment.call(document, text);
      if (node && !('ownerDocument' in node)) {
        try {
          Object.defineProperty(node, 'ownerDocument', {
            get: function() { return document; },
            configurable: true
          });
        } catch(e) {
          node.ownerDocument = document;
        }
      }
      return node;
    };
  }

  // Patcha createDocumentFragment
  var _origCreateFragment = document.createDocumentFragment;
  if (_origCreateFragment) {
    document.createDocumentFragment = function() {
      var frag = _origCreateFragment.call(document);
      if (frag && !('ownerDocument' in frag)) {
        try {
          Object.defineProperty(frag, 'ownerDocument', {
            get: function() { return document; },
            configurable: true
          });
        } catch(e) {
          frag.ownerDocument = document;
        }
      }
      return frag;
    };
  }
})();

// ─── node.compareDocumentPosition — native Rust (register_window) ────────────
// Node-konstanter sätts av dom_bridge.rs

// ─── Element/Node metoder — native Rust (dom_bridge.rs) ─────────────────────
// Syntaktisk socker: el.remove() === el.parentNode.removeChild(el)
// Patcha via document.createElement wrapper
(function() {
  // Vi kan inte patcha alla element, men vi kan lägga till remove() på
  // element som getElementById/querySelector returnerar.
  // Bättre approach: patcha i en MutationObserver-liknande hook.
  // Enklaste: lägg till som polyfill-funktion som WPT-testerna kan använda.
  if (typeof document === 'undefined') return;

  // Konvertera argument till nod per spec: strings/null/undefined/numbers → textNode
  function toNode(arg) {
    if (arg && typeof arg === 'object' && (arg.nodeType || arg.appendChild)) return arg;
    return document.createTextNode(String(arg));
  }

  // Utility: lägg till ChildNode-metoder på ett element-objekt
  function patchChildNode(el) {
    if (!el || typeof el !== 'object') return el;
    // Sätt rätt prototypkedja (instanceof HTMLDivElement etc.)
    if (typeof __patchPrototype === 'function') __patchPrototype(el);
    // CharacterData-metoder för text/comment-noder
    if (typeof __patchCharacterData === 'function') __patchCharacterData(el);

    // remove(), before(), after() — nu Rust-native i dom_bridge.rs

    // after(), replaceWith() — nu Rust-native i dom_bridge.rs

    // prepend, append, replaceChildren — nu Rust-native i dom_bridge.rs

    return el;
  }

  // Gör patchChildNode tillgänglig globalt — runnern kan använda den
  globalThis.__patchChildNode = patchChildNode;

  // Patcha document.createElement
  var _ce = document.createElement;
  document.createElement = function(tag) {
    return patchChildNode(_ce.call(document, tag));
  };

  // Patcha document.getElementById
  var _gid = document.getElementById;
  if (_gid) {
    document.getElementById = function(id) {
      return patchChildNode(_gid.call(document, id));
    };
  }

  // Patcha document.querySelector
  var _qs = document.querySelector;
  if (_qs) {
    document.querySelector = function(sel) {
      return patchChildNode(_qs.call(document, sel));
    };
  }

  // Patcha document.querySelectorAll — returnerar array-like
  var _qsa = document.querySelectorAll;
  if (_qsa) {
    document.querySelectorAll = function(sel) {
      var result = _qsa.call(document, sel);
      if (result && result.length) {
        for (var i = 0; i < result.length; i++) {
          patchChildNode(result[i]);
        }
      }
      return result;
    };
  }

  // Patcha document.createTextNode
  var _ctn = document.createTextNode;
  document.createTextNode = function(text) {
    return patchChildNode(_ctn.call(document, text));
  };

  // Patcha document.createDocumentFragment
  var _cdf = document.createDocumentFragment;
  document.createDocumentFragment = function() {
    return patchChildNode(_cdf.call(document));
  };

  // Patcha document.createComment
  var _cc = document.createComment;
  document.createComment = function(text) {
    return patchChildNode(_cc.call(document, text));
  };
})();

// ─── Element.insertAdjacentElement() ────────────────────────────────────────
// insertAdjacentHTML finns redan — lägg till insertAdjacentElement
// och insertAdjacentText
(function() {
  // Dessa patchas via createElement-wrappern ovan,
  // men vi lägger också till dem i __patchChildNode
  var _origPatch = globalThis.__patchChildNode;
  globalThis.__patchChildNode = function(el) {
    if (!el || typeof el !== 'object') return el;
    el = _origPatch(el);

    // element.attributes — NamedNodeMap (live, Proxy-baserad om tillgänglig)
    if (!el.attributes && el.nodeType === 1 && el.getAttributeNames) {
      Object.defineProperty(el, 'attributes', {
        get: function() {
          var self = this;
          var getAttrsFn = function() {
            var names = self.getAttributeNames ? self.getAttributeNames() : [];
            var map = [];
            var nsAttrs = self.__nsAttrs || {};
            var nsKeys = {};
            Object.keys(nsAttrs).forEach(function(key) {
              var a = nsAttrs[key];
              nsKeys[a.localName] = a;
            });
            for (var i = 0; i < names.length; i++) {
              var n = names[i];
              var v = self.getAttribute(n);
              var ns = nsKeys[n];
              map.push({
                name: ns ? ns.name : n,
                localName: ns ? ns.localName : n,
                value: ns ? ns.value : v,
                namespaceURI: ns ? ns.namespaceURI : null,
                prefix: ns ? ns.prefix : null,
                specified: true, ownerElement: self,
                nodeType: 2, nodeName: ns ? ns.name : n
              });
            }
            Object.keys(nsAttrs).forEach(function(key) {
              var a = nsAttrs[key];
              if (names.indexOf(a.localName) === -1) {
                map.push({
                  name: a.name, localName: a.localName, value: a.value,
                  namespaceURI: a.namespaceURI, prefix: a.prefix,
                  specified: true, ownerElement: self,
                  nodeType: 2, nodeName: a.name
                });
              }
            });
            return map;
          };
          // Använd Proxy-baserad NamedNodeMap om tillgänglig
          if (typeof __createNamedNodeMap === 'function') {
            return __createNamedNodeMap(getAttrsFn, self);
          }
          // Fallback: vanlig array med metoder
          var map = getAttrsFn();
          map.getNamedItem = function(name) {
            for (var j = 0; j < this.length; j++) {
              if (this[j].name === name) return this[j];
            }
            return null;
          };
          map.getNamedItemNS = function(ns, name) {
            for (var j = 0; j < this.length; j++) {
              if (this[j].localName === name && this[j].namespaceURI === ns) return this[j];
            }
            return null;
          };
          map.item = function(idx) { return this[idx] || null; };
          map.setNamedItem = function(attr) {
            self.setAttribute(attr.name, attr.value);
            return null;
          };
          map.removeNamedItem = function(name) {
            var old = self.getAttribute(name);
            self.removeAttribute(name);
            return { name: name, value: old, nodeType: 2, nodeName: name,
                     localName: name, namespaceURI: null, prefix: null };
          };
          return map;
        },
        configurable: true
      });
    }

    // getElementsByTagName, getElementsByClassName, getElementsByTagNameNS
    // — nu Rust-native i dom_bridge.rs

    // moveBefore — nu Rust-native i dom_bridge.rs

    // lookupNamespaceURI, lookupPrefix, isDefaultNamespace
    // — nu Rust-native i dom_bridge.rs

    // Namespace-metoder (NS-varianter)
    if (el.nodeType === 1) {
      // NS-metadata tracking — Rust lagrar värdet, JS spårar prefix/namespace
      if (el.setAttributeNS) {
        var _rustSetNS = el.setAttributeNS;
        el.__nsAttrs = el.__nsAttrs || {};
        el.setAttributeNS = function(ns, qname, val) {
          var parts = qname.split(':');
          var prefix = parts.length > 1 ? parts[0] : null;
          var local = parts.length > 1 ? parts[1] : qname;
          var key = (ns || '') + '|' + local;
          this.__nsAttrs = this.__nsAttrs || {};
          this.__nsAttrs[key] = { namespaceURI: ns, prefix: prefix, localName: local, value: String(val), name: qname };
          return _rustSetNS.call(this, ns, qname, val);
        };
      }
      if (!el.getAttributeNodeNS) {
        el.getAttributeNodeNS = function(ns, local) {
          if (!this.hasAttributeNS(ns, local)) return null;
          return {
            name: local, localName: local, value: this.getAttributeNS(ns, local),
            namespaceURI: ns, prefix: null, specified: true,
            ownerElement: this, nodeType: 2, nodeName: local
          };
        };
      }
    }

    // id, className — måste skriva tillbaka till arena via setAttribute
    if (el.nodeType === 1 && el.setAttribute) {
      var _origId = el.id || '';
      Object.defineProperty(el, 'id', {
        get: function() { return this.getAttribute('id') || ''; },
        set: function(v) { this.setAttribute('id', v); },
        configurable: true
      });
      var _origClass = el.className || '';
      Object.defineProperty(el, 'className', {
        get: function() { return this.getAttribute('class') || ''; },
        set: function(v) { this.setAttribute('class', v); },
        configurable: true
      });
    }

    // prefix, namespaceURI, localName — HTML-element har aldrig prefix/namespace
    if (el.nodeType === 1) {
      if (!('prefix' in el)) el.prefix = null;
      if (!('namespaceURI' in el)) el.namespaceURI = 'http://www.w3.org/1999/xhtml';
      if (!('localName' in el)) {
        el.localName = (el.tagName || '').toLowerCase();
      }
    }

    // toggleAttribute — nu Rust-native i dom_bridge.rs

    // getAttributeNode — nu Rust-native i dom_bridge.rs

    // getAttributeNames()
    if (!el.getAttributeNames && el.getAttribute) {
      // Kan inte implementera utan tillgång till attributlistan — hoppa
    }

    // insertAdjacentElement, insertAdjacentText — nu Rust-native i dom_bridge.rs

    return el;
  };
})();

// ─── DOMException ───────────────────────────────────────────────────────────
// MIGRERAD TILL RUST (2026-03-25) — registreras native via register_dom_exception()
// Polyfill borttagen. DOMException skapas nu i dom_bridge.rs innan polyfills laddas.

// ─── Synkronisera window med globalThis ─────────────────────────────────────
// WPT-tester använder "X in window" — allt på globalThis ska finnas på window
(function() {
  if (typeof window !== 'undefined' && typeof globalThis !== 'undefined') {
    // Gör window till en proxy för globalThis om möjligt
    // (window.HTMLAnchorElement etc. ska fungera)
    var origWindow = window;
    try {
      var handler = {
        get: function(target, prop) {
          if (prop in target) return target[prop];
          if (prop in globalThis) return globalThis[prop];
          return undefined;
        },
        has: function(target, prop) {
          return (prop in target) || (prop in globalThis);
        },
        set: function(target, prop, value) {
          target[prop] = value;
          return true;
        },
        deleteProperty: function(target, prop) {
          delete target[prop];
          delete globalThis[prop];
          return true;
        }
      };
      var proxyWin = new Proxy(origWindow, handler);
      globalThis.window = proxyWin;
      globalThis.self = proxyWin;
    } catch(e) {
      // Proxy inte tillgänglig — kopiera manuellt
    }
  }
})();

// ─── document.createElementNS — native Rust (CreateElementNS) ────────────────
// ─── document.getElementsByTagNameNS — native Rust (GetElementsByTagNameNSDoc) ─

// ─── NodeFilter konstanter — MIGRERAD till window.rs (native) ────────────────

// Range API — native Rust (dom_bridge.rs)
// document.createAttribute — native Rust (CreateAttribute handler)

// ─── Document konstruktor → skapar riktig arena-backed doc ──────────────────
(function() {
  if (typeof document === 'undefined' || !document.implementation) return;
  var _origDocProto = globalThis.Document ? globalThis.Document.prototype : {};
  globalThis.Document = function Document() {
    // new Document() → createHTMLDocument utan titel
    var doc = document.implementation.createHTMLDocument('');
    // Sätt prototype för instanceof
    try { Object.setPrototypeOf(doc, _origDocProto); } catch(e) {}
    return doc;
  };
  globalThis.Document.prototype = _origDocProto;
  globalThis.Document.prototype.constructor = globalThis.Document;

  // XMLDocument — separat konstruktor (INTE subclass av Document)
  // DOMParser spec: parseFromString returnerar Document, INTE XMLDocument
  // Så instanceof XMLDocument === false för DOMParser-resultat
  globalThis.XMLDocument = function XMLDocument() {};
  globalThis.XMLDocument.prototype = {};
  globalThis.XMLDocument.prototype.constructor = globalThis.XMLDocument;
})();

// ─── Patcha document.body/head/documentElement (pre-cache-skapade) ───────────
(function() {
  if (typeof document === 'undefined') return;
  // Dessa element skapades före polyfills — patcha dom nu
  [document.body, document.head, document.documentElement].forEach(function(el) {
    if (el && typeof __patchChildNode === 'function') __patchChildNode(el);
    if (el && typeof __patchPrototype === 'function') __patchPrototype(el);
  });
})();

// ─── NodeList.forEach ───────────────────────────────────────────────────────
// querySelectorAll returnerar array-like, men forEach behövs ofta
(function() {
  if (typeof NodeList === 'undefined') {
    globalThis.NodeList = function() {};
    NodeList.prototype = Object.create(Array.prototype);
  }
})();

// ─── DOM Type Hierarchy + instanceof-stöd ───────────────────────────────────
// MIGRERAD: EventTarget, Node, CharacterData, Element, HTMLElement → native i dom_bridge.rs
// Kvar: HTML element tag → constructor mappning (HTMLDivElement etc.)
(function() {
  // Referera till de native-registrerade bastyperna
  var HTMLElementBase = globalThis.HTMLElement || function() {};
  var CharacterDataBase = globalThis.CharacterData || function() {};
  var NodeBase = globalThis.Node || function() {};

  // Tagnamn → konstruktor-mappning
  var tagMap = {};
  var htmlTypes = {
    'HTMLDivElement': ['div'],
    'HTMLSpanElement': ['span'],
    'HTMLParagraphElement': ['p'],
    'HTMLAnchorElement': ['a'],
    'HTMLButtonElement': ['button'],
    'HTMLInputElement': ['input'],
    'HTMLFormElement': ['form'],
    'HTMLSelectElement': ['select'],
    'HTMLOptionElement': ['option'],
    'HTMLTextAreaElement': ['textarea'],
    'HTMLImageElement': ['img'],
    'HTMLTableElement': ['table'],
    'HTMLTableRowElement': ['tr'],
    'HTMLTableCellElement': ['td', 'th'],
    'HTMLTableSectionElement': ['thead', 'tbody', 'tfoot'],
    'HTMLTableCaptionElement': ['caption'],
    'HTMLTableColElement': ['col', 'colgroup'],
    'HTMLHeadingElement': ['h1', 'h2', 'h3', 'h4', 'h5', 'h6'],
    'HTMLLabelElement': ['label'],
    'HTMLFieldSetElement': ['fieldset'],
    'HTMLLegendElement': ['legend'],
    'HTMLUListElement': ['ul'],
    'HTMLOListElement': ['ol'],
    'HTMLLIElement': ['li'],
    'HTMLDListElement': ['dl'],
    'HTMLPreElement': ['pre', 'listing', 'xmp'],
    'HTMLScriptElement': ['script'],
    'HTMLStyleElement': ['style'],
    'HTMLLinkElement': ['link'],
    'HTMLMetaElement': ['meta'],
    'HTMLBaseElement': ['base'],
    'HTMLBodyElement': ['body'],
    'HTMLHeadElement': ['head'],
    'HTMLHtmlElement': ['html'],
    'HTMLBRElement': ['br'],
    'HTMLHRElement': ['hr'],
    'HTMLIFrameElement': ['iframe'],
    'HTMLCanvasElement': ['canvas'],
    'HTMLVideoElement': ['video'],
    'HTMLAudioElement': ['audio'],
    'HTMLSourceElement': ['source'],
    'HTMLTemplateElement': ['template'],
    'HTMLSlotElement': ['slot'],
    'HTMLDataElement': ['data'],
    'HTMLTimeElement': ['time'],
    'HTMLOutputElement': ['output'],
    'HTMLProgressElement': ['progress'],
    'HTMLMeterElement': ['meter'],
    'HTMLDetailsElement': ['details'],
    'HTMLSummaryElement': ['summary'],
    'HTMLDialogElement': ['dialog'],
    'HTMLEmbedElement': ['embed'],
    'HTMLObjectElement': ['object'],
    'HTMLParamElement': ['param'],
    'HTMLTrackElement': ['track'],
    'HTMLAreaElement': ['area'],
    'HTMLMapElement': ['map'],
    'HTMLOptGroupElement': ['optgroup'],
    'HTMLDataListElement': ['datalist'],
    'HTMLModElement': ['ins', 'del'],
    'HTMLQuoteElement': ['blockquote', 'q'],
    'HTMLTitleElement': ['title'],
    'HTMLFontElement': ['font'],
    'HTMLDirectoryElement': ['dir'],
    'HTMLFrameElement': ['frame'],
    'HTMLFrameSetElement': ['frameset'],
    'HTMLMarqueeElement': ['marquee'],
    'HTMLMenuElement': ['menu'],
    'HTMLPictureElement': ['picture'],
    'HTMLUnknownElement': []
  };

  Object.keys(htmlTypes).forEach(function(name) {
    if (!globalThis[name]) {
      var Ctor = function() {};
      Ctor.prototype = Object.create(HTMLElementBase.prototype);
      Ctor.prototype.constructor = Ctor;
      globalThis[name] = Ctor;
    }
    htmlTypes[name].forEach(function(tag) {
      tagMap[tag.toUpperCase()] = globalThis[name];
    });
  });

  // Icke-HTML-typer
  var nonHtml = {
    'Text': CharacterDataBase,
    'Comment': CharacterDataBase,
    'DocumentFragment': NodeBase,
    'Document': NodeBase,
    'DocumentType': NodeBase,
    'ProcessingInstruction': CharacterDataBase,
    'CDATASection': CharacterDataBase,
    'Attr': NodeBase,
    'XMLDocument': NodeBase
  };
  Object.keys(nonHtml).forEach(function(name) {
    // Alltid uppdatera prototypkedjan, även om konstruktorn redan finns
    var existing = globalThis[name];
    if (!existing || typeof existing !== 'function') {
      var Ctor = function() {};
      Ctor.prototype = Object.create(nonHtml[name].prototype);
      Ctor.prototype.constructor = Ctor;
      globalThis[name] = Ctor;
    } else {
      // Uppdatera existerande konstruktors prototype-kedja
      var parent = nonHtml[name].prototype;
      if (!parent.isPrototypeOf(existing.prototype)) {
        var newProto = Object.create(parent);
        // Kopiera existerande egenskaper
        var props = Object.getOwnPropertyNames(existing.prototype);
        for (var i = 0; i < props.length; i++) {
          if (props[i] !== '__proto__') {
            try {
              var desc = Object.getOwnPropertyDescriptor(existing.prototype, props[i]);
              if (desc) Object.defineProperty(newProto, props[i], desc);
            } catch(e) {}
          }
        }
        newProto.constructor = existing;
        existing.prototype = newProto;
      }
    }
  });

  // Utility-typer
  ['AbortSignal','DOMImplementation','NamedNodeMap','NodeList','HTMLCollection','DOMTokenList','DOMStringMap',
   'CSSStyleDeclaration','Range','Selection','TreeWalker','NodeIterator',
   'NodeFilter','MutationRecord','StaticRange','AbstractRange'
  ].forEach(function(name) {
    if (!globalThis[name]) {
      globalThis[name] = function() {};
      globalThis[name].prototype = {};
      globalThis[name].prototype.constructor = globalThis[name];
    }
  });

  // ─── setPrototypeOf på element-objekt ─────────────────────────────
  // make_element_object skapar plain objects — vi patchas via __patchProto
  globalThis.__tagToConstructor = tagMap;
  globalThis.__patchPrototype = function(el) {
    if (!el || typeof el !== 'object') return el;
    var nt = el.nodeType;
    if (!nt) return el;
    var proto = null;
    if (nt === 1) {
      // Element — välj via tagName
      proto = tagMap[el.tagName] || globalThis.HTMLUnknownElement;
    } else if (nt === 3) {
      proto = globalThis.Text;
    } else if (nt === 8) {
      proto = globalThis.Comment;
    } else if (nt === 9) {
      proto = globalThis.Document;
    } else if (nt === 11) {
      proto = globalThis.DocumentFragment;
    } else if (nt === 10) {
      proto = globalThis.DocumentType;
    }
    if (proto && proto.prototype) {
      try { Object.setPrototypeOf(el, proto.prototype); } catch(e) {}
    }
    return el;
  };
})();
